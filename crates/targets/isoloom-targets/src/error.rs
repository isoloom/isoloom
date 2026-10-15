//! Why a target's files weren't generated: the target isn't possible for the lab, has no
//! generator, or its backend refuses the environment.

use isoloom_spec::model::Target;

/// Why a target's files weren't generated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GenerateError {
    /// The lab's machines have no form this target runs (see `isoloom targets`).
    #[error("{}: not possible for this spec (see `isoloom targets`)", .0.id())]
    NotPossible(Target),
    /// The target is possible, but its backend refuses a feature the lab uses, as the refusal
    /// reads.
    #[error("{}: {what}", target.id())]
    Unsupported {
        /// The target that refuses.
        target: Target,
        /// What it doesn't support, as the refusal reads.
        what: String,
    },
    /// No generator exists for this target yet.
    #[error("{}: no generator yet", .0.id())]
    NoGenerator(Target),
}

impl From<isoloom_render::Error> for GenerateError {
    /// A backend's error as the target's: a refusal or a limit of its own, read the same way.
    fn from(e: isoloom_render::Error) -> GenerateError {
        GenerateError::Unsupported {
            target: e.target(),
            what: e.reason(),
        }
    }
}
