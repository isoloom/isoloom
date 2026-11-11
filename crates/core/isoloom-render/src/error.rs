//! Why a backend didn't render an environment: the one error every backend reports, so the
//! registry (`isoloom-targets`) and the command line read each the same way. A refusal (the
//! target can't run the environment) is kept apart from a failure (a bug of the backend, or a file it
//! can't read): a refusal leaves the target out, a failure stops generation.

use std::sync::Arc;

use isoloom_plan::Refusal;
use isoloom_spec::model::Target;

/// Why a backend didn't render an environment.
#[derive(Debug, Clone, thiserror::Error)]
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
    /// The backend failed, not the environment: a value its own types refused, a file its writer couldn't
    /// write, a project file it couldn't read. Not a refusal: generation stops on it.
    #[error("{}: {what}", target.id())]
    Internal {
        /// The target being rendered.
        target: Target,
        /// What failed (`the run manifest refused a value of this backend`).
        what: String,
        /// The error it failed with.
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
}

impl Error {
    /// A failure of the backend rendering `target`: `what` failed, with `source`.
    pub fn internal(target: Target, what: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Error {
        Error::Internal {
            target,
            what: what.into(),
            source: Arc::new(source),
        }
    }

    /// The target whose backend refused or failed.
    #[must_use]
    pub fn target(&self) -> Target {
        match self {
            Error::Refused { target, .. } | Error::Unsupported { target, .. } | Error::Internal { target, .. } => *target,
        }
    }

    /// Why the target refuses the environment, as the refusal reads after its target; `None` for a
    /// failure of the backend ([`Error::Internal`]), which isn't a refusal.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        match self {
            Error::Refused { refusal, .. } => Some(refusal.to_string()),
            Error::Unsupported { what, .. } => Some(what.clone()),
            Error::Internal { .. } => None,
        }
    }

    /// The same error, reported as `target`'s: a backend that renders part of another target's
    /// files (the `docker` target's Compose file inside `docker-vm`'s VM) refuses or fails as
    /// that target.
    #[must_use]
    pub fn on(self, target: Target) -> Error {
        match self {
            Error::Refused { refusal, .. } => Error::Refused { target, refusal },
            Error::Unsupported { what, .. } => Error::Unsupported { target, what },
            Error::Internal { what, source, .. } => Error::Internal { target, what, source },
        }
    }
}

/// Two errors are the same when they read the same: a failure's cause compares by its message
/// (the error types behind it needn't compare).
impl PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        match (self, other) {
            (Error::Refused { target, refusal }, Error::Refused { target: t, refusal: r }) => (target, refusal) == (t, r),
            (Error::Unsupported { target, what }, Error::Unsupported { target: t, what: w }) => (target, what) == (t, w),
            (Error::Internal { target, what, source }, Error::Internal { target: t, what: w, source: s }) => {
                (target, what, source.to_string()) == (t, w, s.to_string())
            }
            _ => false,
        }
    }
}

impl Eq for Error {}

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
        assert_eq!(unsupported.refusal().as_deref(), Some("machine `w`: no image"));
        assert_eq!(
            refused.refusal().as_deref(),
            Some("machine `aliases` (dotted DNS names) aren't Kubernetes Service names")
        );
    }

    #[test]
    fn a_failure_keeps_its_cause_and_is_no_refusal() {
        use std::error::Error as _;

        let cause = std::io::Error::other("disk full");
        let failed = Error::internal(Target::Docker, "the Compose file couldn't be written", cause).on(Target::DockerVm);
        assert_eq!(failed.to_string(), "docker-vm: the Compose file couldn't be written");
        assert_eq!(failed.source().map(ToString::to_string).as_deref(), Some("disk full"));
        assert_eq!(failed.refusal(), None);
        assert_eq!(failed.target(), Target::DockerVm);
    }
}
