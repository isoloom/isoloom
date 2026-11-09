//! Why a Vagrantfile isn't rendered, and how a backend reports it.

use isoloom_manifest::Error as ManifestError;
use isoloom_plan::Refusal;
use isoloom_render::Error as RenderError;
use isoloom_spec::model::Target;

/// Why a Vagrantfile isn't rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The environment needs what the target can't do (the plan's refusal).
    #[error("{}: {refusal}", target.id())]
    Refused {
        /// The target that refuses.
        target: Target,
        /// The first requirement it can't meet.
        refusal: Refusal,
    },
    /// A machine Vagrant can't build: no box for its OS, a step it can't run, what a Windows
    /// machine can't have yet.
    #[error("{}: {reason}", target.id())]
    Unsupported {
        /// The target that refuses.
        target: Target,
        /// Why, as the refusal reads.
        reason: String,
    },
    /// A value of the run manifest was refused by the manifest's own types: a bug of this
    /// backend, which builds every one of them.
    #[error("{}: {MANIFEST_REFUSED}", target.id())]
    Manifest {
        /// The target whose manifest it is (`vagrant`, unless the error is reported as another target's).
        target: Target,
        /// What the manifest's types refused.
        #[source]
        source: ManifestError,
    },
}

/// What a manifest error is, after its target.
const MANIFEST_REFUSED: &str = "the run manifest refused a value of this backend";

impl From<ManifestError> for Error {
    /// The `vagrant` target's manifest error, until reported as another target's.
    fn from(source: ManifestError) -> Error {
        Error::Manifest {
            target: Target::Vagrant,
            source,
        }
    }
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The error reported as `target`'s: the `hybrid` target's VMs are the `vagrant` target's,
    /// and `docker-vm`'s manifest is built by the same code.
    pub(crate) fn on(self, target: Target) -> Error {
        match self {
            Error::Refused { refusal, .. } => Error::Refused { target, refusal },
            Error::Unsupported { reason, .. } => Error::Unsupported { target, reason },
            Error::Manifest { source, .. } => Error::Manifest { target, source },
        }
    }
}

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, what the target doesn't
    /// support, or a failure of this backend with its cause.
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unsupported { target, reason } => RenderError::Unsupported { target, what: reason },
            Error::Manifest { target, source } => RenderError::internal(target, MANIFEST_REFUSED, source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_of_the_vms_is_the_hybrid_targets_own() {
        let e = Error::Unsupported {
            target: Target::Vagrant,
            reason: "machine `m`: no Vagrant box".to_owned(),
        }
        .on(Target::Hybrid);
        assert_eq!(e.to_string(), "hybrid: machine `m`: no Vagrant box");
        assert_eq!(RenderError::from(e).to_string(), "hybrid: machine `m`: no Vagrant box");
    }

    #[test]
    fn a_manifest_bug_is_a_failure_of_the_target_it_was_built_for() {
        use std::error::Error as _;

        let source = isoloom_manifest::EnvironmentName::parse("Not A Name").expect_err("refused");
        let e = RenderError::from(Error::from(source).on(Target::DockerVm));
        assert_eq!(e.to_string(), format!("docker-vm: {MANIFEST_REFUSED}"));
        assert!(e.source().is_some());
        assert_eq!(e.refusal(), None);
    }
}
