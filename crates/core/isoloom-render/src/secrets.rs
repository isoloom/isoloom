//! Secrets on a VM and on a Docker host. The generated set-up never holds a value: it waits
//! for the runner to deliver them over the machine's own connection (SSH), then puts each
//! file in place. Delivery writes a root-only copy of each value under `KEPT_DIR` (it outlives
//! a reboot, which empties `/run/secrets`) and the mark [`DELIVERED`] the set-up waits for.
//!
//! The order is the plan's `Secrets` step: after the machine's files, before what may read a
//! secret (its `provision` steps, its `before` steps and its command). The set-up runs inside
//! the target's own tool (`vagrant up`, `terraform apply`, cloud-init), so the step is a gate:
//! it waits for the mark, and the runner delivers while the tool runs.

use isoloom_plan::secrets::VM_DEFAULT_MODE;
use isoloom_plan::{Environment, MachineShape, SecretFile};
use isoloom_spec::model::{SecretSource, Spec, secret_variable};
use isoloom_spec::shell::quote;

use crate::boot;
use crate::run_manifest::RUNNER;

pub mod windows;

/// Where a machine keeps the secrets delivered to it: root-only, on disk.
pub(crate) const KEPT_DIR: &str = "/var/lib/isoloom/secrets";
/// The mark delivery leaves once every secret is kept.
pub const DELIVERED: &str = "/var/lib/isoloom/secrets/.delivered";
/// Where a Terraform set-up that waits for its secrets says how to reach its machine, under
/// its module: one file per machine, `<user>@<address>`. The runner reads it while Terraform
/// runs (the module's outputs come only once every set-up is done) and delivers there.
pub const REACH_DIR: &str = ".isoloom-reach";
/// Seconds the set-up waits for the runner to deliver the secrets.
pub(crate) const DELIVERY_TIMEOUT: u32 = 1800;
/// The boot service putting the secrets back in place (`/run` is emptied at every boot). Its
/// name sorts before `before` and `command`, the order OpenRC runs `/etc/local.d` in.
pub(crate) const PLACE_SERVICE: &str = "apply-secrets";

/// The script the runner runs as root on the machine. Its standard input holds one line per
/// secret of `names`, in that order, each value in base64 (so any text fits on one line); the
/// script keeps each one, marks the delivery, then puts `files` in place (so a run again
/// refreshes them). Nothing of a value reaches its command line or its output. The runner's side
/// of delivery, which the runner writes itself: kept here to test the step against.
#[cfg(test)]
fn receive(names: &[String], files: &[SecretFile]) -> String {
    let kept: String = names
        .iter()
        .map(|name| {
            let kept = format!("{KEPT_DIR}/{name}");
            format!("IFS= read -r v || [ -n \"$v\" ]; printf '%s\\n' \"$v\" | base64 -d > {kept}.new; mv -f {kept}.new {kept}\n")
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "set -e\numask 077\nmkdir -p {KEPT_DIR}\nchmod 700 {KEPT_DIR}\n{kept}unset v\ntouch {DELIVERED}\n{}",
        place(files)
    )
}

/// Each kept secret copied to its path, with its owner and mode (root and [`VM_DEFAULT_MODE`]
/// unless the spec says), its folder made when missing.
#[must_use]
pub(crate) fn place(files: &[SecretFile]) -> String {
    files
        .iter()
        .map(|f| {
            format!(
                "install -D -o {} -g {} -m {:04o} {KEPT_DIR}/{} {}\n",
                f.uid.unwrap_or(0),
                f.gid.unwrap_or(0),
                f.mode.unwrap_or(VM_DEFAULT_MODE),
                f.kept(),
                quote(&f.path)
            )
        })
        .collect::<Vec<_>>()
        .concat()
}

/// Waits for the delivery mark, saying so once; fails after `DELIVERY_TIMEOUT` seconds with
/// what to do. `who` names the machine in the messages.
#[must_use]
pub(crate) fn wait(who: &str) -> String {
    let (waiting, missing) = messages(who);
    format!("{}\n", crate::steps::wait_for_mark(DELIVERED, DELIVERY_TIMEOUT, &waiting, &missing))
}

/// What the wait says while it waits, and when it gives up: who delivers (the runner; the
/// target's own tool, Terraform, can't), and what to do.
fn messages(who: &str) -> (String, String) {
    (
        format!("{who}: waiting for its secrets, which {RUNNER} delivers"),
        format!("{who}: its secrets never came; start the environment with {RUNNER}, which reads them from their stores and delivers them"),
    )
}

/// A VM's `Secrets` step: the wait, then its files put in place now and at every boot.
#[must_use]
pub(crate) fn step(machine: &str, files: &[SecretFile]) -> String {
    format!("set -e\n{}{}", wait(machine), placed(files))
}

/// The files put in place now and at every boot.
fn placed(files: &[SecretFile]) -> String {
    boot::at_boot(PLACE_SERVICE, "The machine's secrets, put in place", &place(files), boot::Trigger::Boot, &[])
}

/// A VM's `Secrets` step when the tool that runs it hands the values over itself (Vagrant's
/// provisioner environment, filled from the host's when Vagrant runs): each of `values`, a
/// secret's name and the variable holding its value in base64 (so a `$`, a quote or a line break
/// in it is never read by the shell that sets the variable), kept as delivery keeps it and the
/// delivery marked; then the files put in place now and at every boot. Nothing waits: the
/// values are there. An empty or unset variable writes an empty value the first time, and keeps
/// the value already kept after (a re-provision given no value again).
#[must_use]
pub fn from_environment(values: &[(String, String)], files: &[SecretFile]) -> String {
    let kept = values
        .iter()
        .map(|(name, var)| {
            let kept = format!("{KEPT_DIR}/{name}");
            format!("v=\"$(printenv {var} || true)\"; if [ -n \"$v\" ] || [ ! -e {kept} ]; then printf '%s' \"$v\" | base64 -d > {kept}.new; mv -f {kept}.new {kept}; fi\n")
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "set -e\numask 077\nmkdir -p {KEPT_DIR}\nchmod 700 {KEPT_DIR}\n{kept}touch {DELIVERED}\n{}",
        placed(files)
    )
}

/// The variable the Compose file reads secret `name` from: an input's file, the input's own
/// variable; a secret, `ISOLOOM_SECRET_<NAME>`.
#[must_use]
pub fn compose_variable(spec: &Spec, name: &str) -> String {
    match spec.secrets.get(name) {
        Some(SecretSource::Input { input }) => input.clone(),
        _ => secret_variable(name),
    }
}

/// The name secret `name`'s value is kept under on a machine: an input's file, the input's own
/// name; a secret, its name (as [`SecretFile::kept`]).
fn kept_name<'a>(spec: &'a Spec, name: &'a str) -> &'a str {
    match spec.secrets.get(name) {
        Some(SecretSource::Input { input }) => input,
        _ => name,
    }
}

/// On a Docker host, before `docker compose up`: the wait, then each secret of `names` in the
/// variable the Compose file reads it from ([`compose_variable`]), its last line breaks kept.
/// Nothing when there are none.
#[must_use]
pub fn host_environment(spec: &Spec, names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let read: String = names
        .iter()
        .map(|name| {
            let var = compose_variable(spec, name);
            let kept = kept_name(spec, name);
            format!("{var}=\"$(cat {KEPT_DIR}/{kept}; printf x)\"; {var}=\"${{{var}%x}}\"; export {var}\n")
        })
        .collect::<Vec<_>>()
        .concat();
    wait("the Docker host") + &read
}

/// The secrets a Docker host gets: those its containers get, in the order the spec declares
/// them.
#[must_use]
pub fn of_containers(env: &Environment) -> Vec<String> {
    env.spec()
        .secrets
        .keys()
        .filter(|name| {
            env.machines
                .values()
                .any(|m| m.shape == MachineShape::Container && m.secrets.iter().any(|s| &s.name == *name))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
