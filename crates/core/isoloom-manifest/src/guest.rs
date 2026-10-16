//! Containers run by Compose inside one of the run's VMs (`docker-vm`): the VM's driver makes the
//! VM, the steps run Compose in it, and the runner reads the containers there.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::names::Name;
use crate::step::Step;
use crate::template::Template;

/// The VM the lab's containers run in, and how the runner reads them there: its own machines are
/// the VM's driver's (Vagrant, Terraform), the containers' are Compose's inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Guest {
    /// The VM: a machine of the manifest. Every machine named by a Compose service runs in it.
    pub machine: Name,
    /// Where the containers' published ports answer, when the VM's driver can't say it
    /// (Terraform: `${outputs.ip}`, the server VM's address). Absent: where the driver says the
    /// VM answers (Vagrant: this computer's loopback, to which it forwards the ports; the VM's own
    /// address on a provider that forwards nothing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Template>,
    /// Reads the containers inside the VM, once it runs: `docker compose ps --all --format json`
    /// (an argv through the VM's tool, or `ssh.run`). Never elevated, always time-bounded; an
    /// empty output means no container.
    pub status: Step,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guest_names_its_vm_and_reads_its_containers() {
        let text =
            r#"{"machine":"docker","host":"${outputs.ip}","status":{"id":"ps","action":"ssh.run","with":{"command":"docker ps","host":"${outputs.ip}"}}}"#;
        let g: Guest = serde_json::from_str(text).unwrap();
        assert_eq!(g.machine.as_str(), "docker");
        assert_eq!(g.host.as_ref().map(ToString::to_string).as_deref(), Some("${outputs.ip}"));
        assert_eq!(serde_json::to_string(&g).unwrap(), text);
        let local: Guest = serde_json::from_str(r#"{"machine":"docker","status":{"id":"ps","argv":["vagrant","ssh"]}}"#).unwrap();
        assert_eq!(local.host, None);
        assert!(serde_json::from_str::<Guest>(r#"{"machine":"docker","status":{"id":"ps","argv":["vagrant"]},"shell":true}"#).is_err());
    }
}
