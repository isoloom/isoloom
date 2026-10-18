//! Why a backend didn't render an environment: the one error every backend reports, so the
//! registry (`isoloom-targets`) and the command line read each the same way.

use isoloom_plan::Refusal;
use isoloom_spec::model::Target;

/// Why a backend didn't render an environment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The environment needs a capability the backend lacks (the plan's refusal).
    #[error("{}: {refusal}", target.id())]
    Refused {
        /// The backend's target.
        target: Target,
        /// The requirement it can't meet.
        refusal: Refusal,
    },
    /// The backend refuses the environment for a limit of its own target.
    #[error("{}: {what}", target.id())]
    Unsupported {
        /// The target that refuses.
        target: Target,
        /// What it doesn't support, as the refusal reads.
        what: String,
    },
}

impl Error {
    /// The target whose backend refused.
    #[must_use]
    pub fn target(&self) -> Target {
        match self {
            Error::Refused { target, .. } | Error::Unsupported { target, .. } => *target,
        }
    }

    /// Why, as the refusal reads after its target.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Error::Refused { refusal, .. } => refusal.to_string(),
            Error::Unsupported { what, .. } => what.clone(),
        }
    }
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use isoloom_plan::{Refusal, Requirement};
    use isoloom_spec::model::Target;

    use super::Error;

    #[test]
    fn each_error_reads_as_its_target_then_why() {
        let refused = Error::Refused {
            target: Target::Kubernetes,
            refusal: Refusal {
                requirement: Requirement::MachineAliases,
            },
        };
        assert_eq!(
            refused.to_string(),
            "kubernetes: machine `aliases` (dotted DNS names) aren't Kubernetes Service names"
        );
        let unsupported = Error::Unsupported {
            target: Target::Proxmox,
            what: "machine `w`: no image".to_owned(),
        };
        assert_eq!(unsupported.to_string(), "proxmox: machine `w`: no image");
        assert_eq!((refused.target(), unsupported.target()), (Target::Kubernetes, Target::Proxmox));
        assert_eq!(unsupported.reason(), "machine `w`: no image");
        assert_eq!(refused.reason(), "machine `aliases` (dotted DNS names) aren't Kubernetes Service names");
    }
}
