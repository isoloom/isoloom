//! Why a Vagrantfile isn't rendered, and how a backend reports it.

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
    /// machine can't have.
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

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, or what the target doesn't
    /// support.
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unsupported { target, reason } => RenderError::Unsupported { target, what: reason },
        }
    }
}
