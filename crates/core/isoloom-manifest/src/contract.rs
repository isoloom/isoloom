//! The names the generated files and a runner share outside `run.json`: the variables a
//! Vagrantfile reads, the derived checks' switch, the variable each secret is handed over in, the
//! variables an access machine's image or box is picked with, the tags a cloud resource carries,
//! the paths a machine's set-up writes, the folder every output is under. A runner uses them as
//! they are; Isoloom's backends write them from here, so the two can't drift. Each is part of the
//! contract as the run manifest's fields are: a rename is a breaking change.

/// The folder Isoloom writes a project's outputs under (`.isoloom/<target>/`), and the start of
/// every instance's own (`.isoloom-<n>/`): a run manifest's folder is under one.
pub const OUTPUT_DIR: &str = ".isoloom";

/// What every name Isoloom reserves starts with: a machine, alias or network the spec names
/// can't start with it, and the names Isoloom adds (its controller, its router, its tags, a
/// Kubernetes Service) do.
pub const RESERVED_PREFIX: &str = "isoloom-";

/// The machine that runs a run's controller (Ansible, the playbook checks): its Vagrant
/// `define` name, its name in the manifest's machines.
pub const CONTROLLER: &str = "isoloom-controller";

/// The file a Vagrant controller holds while it powers itself off after its play: the runner
/// takes the controller's going down then as its own doing, not a failure.
pub const HALTING: &str = "/run/isoloom-halting";

/// The key-value tag holding a cloud resource's expiry, in Unix seconds (on the clouds whose
/// tags are keys and values).
pub const EXPIRES_TAG: &str = "isoloom-expires-at";

/// The key-value tag naming the environment (its instance's name) a cloud resource belongs to.
pub const INSTANCE_TAG: &str = "isoloom-instance";

/// The start of the word tag holding a resource's expiry on the clouds whose tags are words
/// (DigitalOcean, Linode): `isoloom-expires-<unix seconds>`. The instance's word is the other
/// one that starts with [`RESERVED_PREFIX`].
pub const EXPIRES_WORD: &str = "isoloom-expires-";

/// The variable a Vagrantfile reads as the ESXi host its VMs go to (`vmware_esxi`); a run
/// manifest's param.
pub const ESXI_HOSTNAME: &str = "ESXI_HOSTNAME";

/// The variable a Vagrantfile reads as the ESXi host's SSH port; a run manifest's param,
/// [`ESXI_DEFAULT_SSH_PORT`] when unset.
pub const ESXI_HOSTPORT: &str = "ESXI_HOSTPORT";

/// The ESXi host's SSH port when [`ESXI_HOSTPORT`] doesn't give one.
pub const ESXI_DEFAULT_SSH_PORT: u16 = 22;

/// The variable a Vagrantfile reads as the command Vagrant's SSH to the guests goes through
/// (`config.ssh.proxy_command`, e.g. `/usr/bin/nc %h %p`), when it is set.
pub const SSH_PROXY_COMMAND: &str = "ISOLOOM_SSH_PROXY_COMMAND";

/// The variable naming the host ports a Vagrant run forwards from this computer, as a JSON
/// object `{"<machine>/<port>": <host port>}`: the Vagrantfile reads it, each port defaulting to
/// its `publish` number.
pub const HOSTPORTS: &str = "ISOLOOM_HOSTPORTS";

/// The variable holding the graphical consoles the runner chooses for a Vagrant run's VMs
/// ([`crate::ConsoleSource::Runner`]), as a JSON object `{"<machine>": {"port": <loopback port>,
/// "password": "<8 characters>"}}`: the Vagrantfile reads it, and turns on a machine's console
/// only when the machine is in it. It holds passwords: a runner gives it to the run's own Vagrant
/// processes only, never to a command an embedder runs.
pub const CONSOLES: &str = "ISOLOOM_CONSOLES";

/// The param naming the host address a Compose file's published ports listen on (`127.0.0.1`
/// when unset; `0.0.0.0` inside a VM that forwards them).
pub const PUBLISH_ADDRESS: &str = "ISOLOOM_PUBLISH_ADDRESS";

/// The param (`1`) asking each published port for its own `publish` number rather than a free
/// one: a Compose file's and a Vagrantfile's.
pub const PUBLISH_FIXED: &str = "ISOLOOM_PUBLISH_FIXED";

/// The switch of the checks derived from the spec (reachability, no internet): the check
/// runners read it, `0` skipping them, and every target passes it through from the host; a run
/// manifest's param.
pub const DERIVED: &str = "ISOLOOM_DERIVED";

/// The start of the variable a secret's value is handed over in, to the tool that delivers it
/// ([`secret_variable`]).
pub const SECRET_PREFIX: &str = "ISOLOOM_SECRET_";

/// The variable secret `name`'s value is handed over in: [`SECRET_PREFIX`] and the name in upper
/// snake case (`ISOLOOM_SECRET_DB_PASSWORD` for `db-password`). Secret names are kebab-case, so
/// two secrets never share one.
#[must_use]
pub fn secret_variable(name: &str) -> String {
    format!("{SECRET_PREFIX}{}", name.to_ascii_uppercase().replace('-', "_"))
}

/// The variable the access machine's Compose service reads its image from (its default, the
/// resolved access image, is the Compose `${VAR:-default}` fallback). The runner sets it to the
/// image it was given when it hot-adds the machine ([`crate::access`]).
pub const ACCESS_IMAGE: &str = "ISOLOOM_ACCESS_IMAGE";

/// The variable the access machine's Vagrant VM reads its box name from (its default is the
/// `ENV.fetch` fallback). The runner sets it to the box it was given when it hot-adds the
/// machine ([`crate::access`]).
pub const ACCESS_BOX: &str = "ISOLOOM_ACCESS_BOX";

/// The folder on a Linux machine its guest-delivered secrets are kept in, one root-only file per
/// slot.
pub const SECRETS_DIR: &str = "/var/lib/isoloom/secrets";

/// The mark a Linux machine's set-up waits for: every guest-delivered secret is in
/// [`SECRETS_DIR`].
pub const DELIVERED: &str = "/var/lib/isoloom/secrets/.delivered";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_isoloom_adds_are_reserved_and_the_mark_is_in_the_secrets_folder() {
        for name in [CONTROLLER, EXPIRES_TAG, INSTANCE_TAG, EXPIRES_WORD] {
            assert!(name.starts_with(RESERVED_PREFIX), "{name}");
        }
        assert!(EXPIRES_TAG.starts_with(EXPIRES_WORD));
        assert_eq!(DELIVERED, format!("{SECRETS_DIR}/.delivered"));
        assert!(OUTPUT_DIR.starts_with('.') && RESERVED_PREFIX.starts_with(OUTPUT_DIR.trim_start_matches('.')));
    }

    #[test]
    fn the_runners_variables_are_named_as_released() {
        assert_eq!(DERIVED, "ISOLOOM_DERIVED");
        assert_eq!(ACCESS_IMAGE, "ISOLOOM_ACCESS_IMAGE");
        assert_eq!(ACCESS_BOX, "ISOLOOM_ACCESS_BOX");
        assert_eq!(secret_variable("db-password"), "ISOLOOM_SECRET_DB_PASSWORD");
        assert!(secret_variable("db-password").starts_with(SECRET_PREFIX));
    }
}
