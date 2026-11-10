//! The names the generated files and a runner share outside `run.json`: the variables a
//! Vagrantfile reads, the tags a cloud resource carries, the paths a machine's set-up writes, the
//! folder every output is under. A runner uses them as they are; Isoloom's backends write them
//! from here, so the two can't drift. Each is part of the contract as the run manifest's fields
//! are: a rename is a breaking change.

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

/// The param (`1`) asking each published port for its own `publish` number rather than a free
/// one: a Compose file's and a Vagrantfile's.
pub const PUBLISH_FIXED: &str = "ISOLOOM_PUBLISH_FIXED";

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
}
