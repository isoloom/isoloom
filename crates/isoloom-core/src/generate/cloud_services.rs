//! The `cloud-services` target: an environment of cloud services (`cloud:`), its Terraform
//! module applied into the user's own cloud account.
//!
//! The module is the project's own and stays untouched: Isoloom writes, under
//! `.isoloom/cloud-services/`, its variables (`terraform.tfvars.json`) and three scripts that run
//! Terraform on a copy of it (`module/`) with the state and the provider plugins kept here, not
//! in the project: `up.sh` (the copy, the `before` steps, init and apply), `down.sh` (destroy)
//! and `outputs.sh` (the outputs, as JSON). The cloud's credentials come from the environment,
//! as Terraform expects.

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::model::Spec;
use crate::shell;

const DIR: &str = "cloud-services";
/// From `.isoloom/cloud-services/` back to the project folder.
const ROOT: &str = "../..";

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    let cloud = spec.cloud.as_ref().expect("the cloud-services target needs cloud:");
    let src = shell::quote(&format!("{ROOT}/{}", cloud.terraform.trim_end_matches('/')));
    // Fixed values only: a variable taking a launch-time input is written by `run`, from the
    // environment, to inputs.tfvars.json next to the state (kept for `down`).
    let vars: serde_json::Map<String, serde_json::Value> = cloud
        .vars
        .iter()
        .filter(|(_, v)| crate::model::CloudServices::inputs_in(v).is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut vars_json = serde_json::to_string_pretty(&serde_json::Value::Object(vars)).expect("vars serialize");
    vars_json.push('\n');

    // Every script starts in its own folder, keeps Terraform's plugins and state there, and runs
    // Terraform on its copy of the module (`-chdir`), whatever the caller's working directory.
    let mut env = String::new();
    for (k, v) in &cloud.env {
        env.push_str(&format!("export {k}={}\n", shell::quote(v)));
    }
    let prelude = |what: &str| {
        let mut s = String::from("#!/bin/sh\n");
        s.push_str(&header("#"));
        s.push_str(&format!("# {what}\n"));
        s.push_str(&format!(
            "# The services of `{}`: the Terraform module `{}`, in the user's {} account.\nset -eu\ncd \"$(dirname \"$0\")\"\nhere=\"$(pwd)\"\nexport TF_DATA_DIR=\"$here/.terraform\"\n{env}src={src}\nmodule=\"$here/module\"\n",
            spec.name,
            cloud.terraform,
            cloud.provider.id()
        ));
        s
    };
    // The module is copied next to the state, and Terraform runs there: its lock file and what
    // the module writes (archives, keys) stay out of the project. A copy refreshed over the old
    // one, so what the steps or Terraform wrote there before stays; the old copy's Terraform
    // files go first, so a file renamed or deleted in the module isn't applied any more.
    let mut copy = String::from(
        "mkdir -p \"$module\"\nfind \"$module\" -type f \\( -name '*.tf' -o -name '*.tf.json' -o -name '*.tfvars' -o -name '*.tfvars.json' \\) -exec rm -f {} +\ntar -C \"$src\" -cf - --exclude=.terraform --exclude='*.tfstate' --exclude='*.tfstate.*' . | tar -C \"$module\" -xf -\n",
    );
    // The host steps, in order, from the copy (a script runs as itself when executable, else
    // with sh).
    if !cloud.before.is_empty() {
        copy.push_str("export ISOLOOM_PROJECT=\"$(cd ../.. && pwd)\"\n");
        copy.push_str("step() { echo \"isoloom: before: $1\" >&2; if [ -x \"$ISOLOOM_PROJECT/$1\" ]; then (cd \"$module\" && \"$ISOLOOM_PROJECT/$1\"); else (cd \"$module\" && sh \"$ISOLOOM_PROJECT/$1\"); fi; }\n");
        for b in &cloud.before {
            copy.push_str(&format!("step {}\n", shell::quote(b)));
        }
    }
    let init = "terraform -chdir=\"$module\" init -input=false >&2\n";
    // The fixed variables, then the launch-time inputs `run` wrote (when the spec takes any).
    let files =
        "set -- -var-file=\"$here/terraform.tfvars.json\"\n[ ! -f \"$here/inputs.tfvars.json\" ] || set -- \"$@\" -var-file=\"$here/inputs.tfvars.json\"\n";
    let state = "-state=\"$here/terraform.tfstate\"";
    // While it applies, `deploying` holds this script's process id (`isoloom status` reads it).
    let marker = "echo $$ > \"$here/deploying\"\ntrap 'rm -f \"$here/deploying\"' EXIT\n";
    let apply = format!("terraform -chdir=\"$module\" apply -auto-approve -input=false {state} \"$@\"");
    let apply = match cloud.apply_retries {
        0 => format!("{apply}\n"),
        n => format!(
            "tries=0\nwhile :; do\n  rc=0\n  {apply} || rc=$?\n  [ \"$rc\" -ne 0 ] || break\n  tries=$((tries + 1))\n  [ \"$tries\" -le {n} ] || exit \"$rc\"\n  echo \"isoloom: the apply failed; applying again ($tries of {n})\" >&2\ndone\n"
        ),
    };
    let up = format!(
        "{}{marker}{copy}{init}{files}{apply}",
        prelude("Creates the services (`isoloom run cloud-services`).")
    );
    // Destroying needs the module as it was applied (and what the steps wrote there): the copy,
    // made again only when it is gone.
    let down = format!(
        "{}if [ ! -d \"$module\" ]; then\n{}fi\n{init}{files}terraform -chdir=\"$module\" destroy -auto-approve -input=false {state} \"$@\"\n",
        prelude("Destroys them (`isoloom down cloud-services`): what they cost stops here."),
        copy.lines().map(|l| format!("  {l}\n")).collect::<String>()
    );
    let outputs = format!(
        "{}[ -d \"$module\" ] || module=\"$src\"\nterraform -chdir=\"$module\" output -json {state}\n",
        prelude("Prints the module's outputs as JSON (what `isoloom test` and the message read).")
    );
    let path = |f: &str| format!("{OUTPUT_DIR}/{DIR}/{f}");
    Ok(vec![
        GeneratedFile {
            path: path("terraform.tfvars.json"),
            contents: vars_json,
        },
        GeneratedFile {
            path: path("up.sh"),
            contents: up,
        },
        GeneratedFile {
            path: path("down.sh"),
            contents: down,
        },
        GeneratedFile {
            path: path("outputs.sh"),
            contents: outputs,
        },
    ])
}
