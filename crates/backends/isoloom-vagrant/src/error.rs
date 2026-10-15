//! Why a Vagrantfile isn't rendered, and how a backend reports it.

use isoloom_plan::Refusal;
use isoloom_render::GenerateError;
use isoloom_render::backend::RenderError;
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
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The error reported as `target`'s: the `hybrid` target's VMs are the `vagrant` target's.
    pub(crate) fn on(self, target: Target) -> Error {
        match self {
            Error::Refused { refusal, .. } => Error::Refused { target, refusal },
            Error::Unsupported { reason, .. } => Error::Unsupported { target, reason },
        }
    }
}

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, or what the target doesn't
    /// support.
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unsupported { target, reason } => RenderError::Generate(GenerateError::Unsupported { target, what: reason }),
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
}
