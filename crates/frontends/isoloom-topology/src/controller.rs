//! Isoloom's controller: a Windows machine's own steps are PowerShell scripts, so the Ansible
//! playbooks drawn on one run from the controller Isoloom adds (`provision:`), which reaches it
//! over WinRM, as Isoloom's own Windows examples do.

use crate::document::ProvisionDoc;

/// A playbook the controller runs, and the machine it was drawn on (the one it sets up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControllerPlaybook {
    /// The playbook's path in the project.
    pub(crate) path: String,
    /// The machine whose provisioning step it is.
    pub(crate) machine: String,
}

impl From<&ControllerPlaybook> for ProvisionDoc {
    fn from(p: &ControllerPlaybook) -> ProvisionDoc {
        ProvisionDoc { ansible: p.path.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_controller_playbook_is_a_provision_step() {
        let p = ControllerPlaybook {
            path: "provision/dc01.yml".into(),
            machine: "dc01".into(),
        };
        assert_eq!(
            ProvisionDoc::from(&p),
            ProvisionDoc {
                ansible: "provision/dc01.yml".into()
            }
        );
    }
}
