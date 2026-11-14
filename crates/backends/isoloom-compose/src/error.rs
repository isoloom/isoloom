//! Why a Compose file isn't rendered (or read back), and how a backend reports it.

use isoloom_plan::Refusal;
use isoloom_render::Error as RenderError;
use isoloom_spec::model::Target;

/// Why a Compose file isn't rendered, or read back.
#[derive(Debug, thiserror::Error)]
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
    /// The Compose file couldn't be written as YAML.
    #[error("{}: {UNWRITABLE}", target.id())]
    Unwritable {
        /// The target whose file it is.
        target: Target,
        /// What the YAML writer reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, or a failure of this backend
    /// with its cause. The manifest's is the `docker` target's: a target running the Compose
    /// file inside its VM reports it as its own ([`RenderError::on`]).
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unwritable { target, source } => RenderError::internal(target, UNWRITABLE, source),
        }
    }
}

/// What an [`Error::Unwritable`] is, after its target.
const UNWRITABLE: &str = "the Compose file couldn't be written as YAML";

#[cfg(test)]
mod tests {
    use isoloom_plan::Requirement;

    use super::*;

    #[test]
    fn a_refusal_reads_the_same_as_the_backend_reports_it() {
        let refusal = Refusal {
            requirement: Requirement::EnvironmentProvisioning,
        };
        let e = Error::Refused {
            target: Target::Hosted,
            refusal: refusal.clone(),
        };
        assert_eq!(e.to_string(), format!("hosted: {refusal}"));
        assert_eq!(RenderError::from(e).to_string(), format!("hosted: {refusal}"));
    }
}
