//! The pieces of the cloud-services scripts that come from `cloud:` beyond the module and its
//! variables: the environment (`env`, the deployment's name), the module's copy and the
//! `before` steps, the apply's marker and its retries (`apply_retries`).

use isoloom_spec::model::CloudServices;
use isoloom_spec::shell;

/// The module is copied next to the state, and Terraform runs there: its lock file and what the
/// module writes (archives, keys) stay out of the project. A copy refreshed over the old one,
/// so what the steps or Terraform wrote there before stays; the old copy's Terraform files go
/// first, so a file renamed or deleted in the module isn't applied any more.
const COPY: &str = "mkdir -p \"$module\"\nfind \"$module\" -type f \\( -name '*.tf' -o -name '*.tf.json' -o -name '*.tfvars' -o -name '*.tfvars.json' \\) -exec rm -f {} +\ntar -C \"$src\" -cf - --exclude=.terraform --exclude='*.tfstate' --exclude='*.tfstate.*' . | tar -C \"$module\" -xf -\n";

/// A `before` step, from the copy: as itself when executable (its shebang), else with `sh`.
const STEP: &str = "export ISOLOOM_PROJECT=\"$(cd ../.. && pwd)\"\nstep() { echo \"isoloom: before: $1\" >&2; if [ -x \"$ISOLOOM_PROJECT/$1\" ]; then (cd \"$module\" && \"$ISOLOOM_PROJECT/$1\"); else (cd \"$module\" && sh \"$ISOLOOM_PROJECT/$1\"); fi; }\n";

/// The name of this deployment (`isoloom run` writes it): the module can tag what it creates
/// with it (`var.isoloom_instance`), so `down` lists what is left with that tag.
const TAG: &str = "[ ! -f \"$here/isoloom-instance\" ] || export TF_VAR_isoloom_instance=\"$(cat \"$here/isoloom-instance\")\"\n";

/// While it applies, `deploying` holds the script's process id (`isoloom status` reads it).
pub(crate) const MARKER: &str = "echo $$ > \"$here/deploying\"\ntrap 'rm -f \"$here/deploying\"' EXIT\n";

/// `outputs.sh` reads the state with the copy as applied, or the module when there is no copy.
pub(crate) const OUTPUTS_MODULE: &str = "[ -d \"$module\" ] || module=\"$src\"\n";

/// What every script sets after `here`: `env`'s variables, the deployment's name, the module
/// (`src`) and its copy (`module`). `src` is the module's folder, quoted, from the scripts'.
pub(crate) fn environment(cloud: &CloudServices, src: &str) -> String {
    let exports = cloud
        .env
        .iter()
        .map(|(k, v)| format!("export {k}={}\n", shell::quote(v)))
        .collect::<Vec<_>>()
        .concat();
    format!("{exports}{TAG}src={src}\nmodule=\"$here/module\"\n")
}

/// The module's copy, then the `before` steps in order.
pub(crate) fn copy(cloud: &CloudServices) -> String {
    if cloud.before.is_empty() {
        return COPY.to_owned();
    }
    let steps = cloud.before.iter().map(|b| format!("step {}\n", shell::quote(b))).collect::<Vec<_>>().concat();
    format!("{COPY}{STEP}{steps}")
}

/// Destroying needs the module as it was applied (and what the steps wrote there): the copy,
/// made again only when it is gone.
pub(crate) fn copy_if_gone(cloud: &CloudServices) -> String {
    let indented = copy(cloud).lines().map(|l| format!("  {l}\n")).collect::<Vec<_>>().concat();
    format!("if [ ! -d \"$module\" ]; then\n{indented}fi\n")
}

/// `apply`, a Terraform apply line (without its line end), run again up to `apply_retries`
/// times after it fails; the last failure's exit code is the script's.
pub(crate) fn with_retries(apply: &str, retries: u8) -> String {
    match retries {
        0 => format!("{apply}\n"),
        n => format!(
            "tries=0\nwhile :; do\n  rc=0\n  {apply} || rc=$?\n  [ \"$rc\" -ne 0 ] || break\n  tries=$((tries + 1))\n  [ \"$tries\" -le {n} ] || exit \"$rc\"\n  echo \"isoloom: the apply failed; applying again ($tries of {n})\" >&2\ndone\n"
        ),
    }
}
