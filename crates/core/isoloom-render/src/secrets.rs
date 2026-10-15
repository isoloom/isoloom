//! Secrets on a VM and on a Docker host. The generated set-up never holds a value: it waits
//! for `isoloom run` to deliver them over the machine's own connection (SSH), then puts each
//! file in place. Delivery writes a root-only copy of each value under [`KEPT_DIR`] (it outlives
//! a reboot, which empties `/run/secrets`) and the mark [`DELIVERED`] the set-up waits for.
//!
//! The order is the plan's `Secrets` step: after the machine's files, before what may read a
//! secret (its `provision` steps, its `before` steps and its command). The set-up runs inside
//! the target's own tool (`vagrant up`, `terraform apply`, cloud-init), so the step is a gate:
//! it waits for the mark, and `isoloom run` delivers while the tool runs.

use std::fmt::Write as _;

use isoloom_plan::secrets::VM_DEFAULT_MODE;
use isoloom_plan::{Environment, MachineShape, SecretFile};
use isoloom_spec::model::{SecretSource, Spec, secret_variable};

use crate::boot;
use crate::shell::quote;

pub mod windows;

/// Where a machine keeps the secrets delivered to it: root-only, on disk.
pub const KEPT_DIR: &str = "/var/lib/isoloom/secrets";
/// The mark delivery leaves once every secret is kept.
pub const DELIVERED: &str = "/var/lib/isoloom/secrets/.delivered";
/// Where a Terraform set-up that waits for its secrets says how to reach its machine, under
/// its module: one file per machine, `<user>@<address>`. `isoloom run` reads it while Terraform
/// runs (the module's outputs come only once every set-up is done) and delivers there.
pub const REACH_DIR: &str = ".isoloom-reach";
/// Seconds the set-up waits for `isoloom run` to deliver the secrets.
pub const DELIVERY_TIMEOUT: u32 = 1800;
/// The boot service putting the secrets back in place (`/run` is emptied at every boot). Its
/// name sorts before `before` and `command`, the order OpenRC runs `/etc/local.d` in.
pub const PLACE_SERVICE: &str = "apply-secrets";

/// The script `isoloom run` runs as root on the machine. Its standard input holds one line per
/// secret of `names`, in that order, each value in base64 (so any text fits on one line); the
/// script keeps each one, marks the delivery, then puts `files` in place (so a run again
/// refreshes them). Nothing of a value reaches its command line or its output.
pub fn receive(names: &[String], files: &[SecretFile]) -> String {
    let mut sh = format!("set -e\numask 077\nmkdir -p {KEPT_DIR}\nchmod 700 {KEPT_DIR}\n");
    for name in names {
        let kept = format!("{KEPT_DIR}/{name}");
        let _ = writeln!(
            sh,
            "IFS= read -r v || [ -n \"$v\" ]; printf '%s\\n' \"$v\" | base64 -d > {kept}.new; mv -f {kept}.new {kept}"
        );
    }
    let _ = writeln!(sh, "unset v\ntouch {DELIVERED}");
    sh.push_str(&place(files));
    sh
}

/// Each kept secret copied to its path, with its owner and mode (root and [`VM_DEFAULT_MODE`]
/// unless the spec says), its folder made when missing.
pub fn place(files: &[SecretFile]) -> String {
    files.iter().fold(String::new(), |mut sh, f| {
        let _ = writeln!(
            sh,
            "install -D -o {} -g {} -m {:04o} {KEPT_DIR}/{} {}",
            f.uid.unwrap_or(0),
            f.gid.unwrap_or(0),
            f.mode.unwrap_or(VM_DEFAULT_MODE),
            f.name,
            quote(&f.path)
        );
        sh
    })
}

/// Waits for the delivery mark, saying so once; fails after [`DELIVERY_TIMEOUT`] seconds with
/// what to do. `who` names the machine in the messages.
pub fn wait(who: &str) -> String {
    let waiting = quote(&format!("{who}: waiting for `isoloom run` to deliver its secrets"));
    let missing = quote(&format!(
        "{who}: its secrets never came; start the environment with `isoloom run`, which reads them from their stores and delivers them"
    ));
    let tries = DELIVERY_TIMEOUT / 5;
    format!(
        "n=0; until [ -f {DELIVERED} ]; do [ \"$n\" -gt 0 ] || echo {waiting}; n=$((n+1)); if [ \"$n\" -gt {tries} ]; then echo {missing} >&2; exit 1; fi; sleep 5; done\n"
    )
}

/// A VM's `Secrets` step: the wait, then its files put in place now and at every boot.
pub fn step(machine: &str, files: &[SecretFile]) -> String {
    let service = boot::at_boot(PLACE_SERVICE, "The machine's secrets, put in place", &place(files), false, &[]);
    format!("set -e\n{}{service}", wait(machine))
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

/// On a Docker host, before `docker compose up`: the wait, then each secret of `names` in the
/// variable the Compose file reads it from ([`compose_variable`]), its last line breaks kept.
/// Nothing when there are none.
#[must_use]
pub fn host_environment(spec: &Spec, names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let mut sh = wait("the Docker host");
    for name in names {
        let var = compose_variable(spec, name);
        let _ = writeln!(sh, "{var}=\"$(cat {KEPT_DIR}/{name}; printf x)\"; {var}=\"${{{var}%x}}\"; export {var}");
    }
    sh
}

/// The secrets a Docker host gets: those its containers get, in the order the spec declares
/// them.
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
