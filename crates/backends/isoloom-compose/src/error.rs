//! Why a Compose file isn't rendered (or read back), and how a backend reports it.

use isoloom_manifest::Error as ManifestError;
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
    /// The containers of a planned environment couldn't be planned again on their own (the
    /// `hybrid` target's): a bug, since they planned with the rest.
    #[error("{}: {source}", target.id())]
    Unplanned {
        /// The target whose containers they are.
        target: Target,
        /// Why the plan failed.
        #[source]
        source: isoloom_plan::PlanError,
    },
    /// A value of the run manifest was refused by the manifest's own types: a bug of this
    /// backend, which builds every one of them.
    #[error("{}: {MANIFEST_REFUSED}", Target::Docker.id())]
    Manifest {
        /// What the manifest's types refused.
        #[source]
        source: ManifestError,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<ManifestError> for Error {
    fn from(source: ManifestError) -> Error {
        Error::Manifest { source }
    }
}

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, or a failure of this backend
    /// with its cause. The manifest's is the `docker` target's: a target running the Compose
    /// file inside its VM reports it as its own ([`RenderError::on`]).
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unwritable { target, source } => RenderError::internal(target, UNWRITABLE, source),
            Error::Unplanned { target, source } => RenderError::internal(target, "the containers couldn't be planned", source),
            Error::Manifest { source } => RenderError::internal(Target::Docker, MANIFEST_REFUSED, source),
        }
    }
}

/// What an [`Error::Unwritable`] is, after its target.
const UNWRITABLE: &str = "the Compose file couldn't be written as YAML";
/// What an [`Error::Manifest`] is, after its target.
const MANIFEST_REFUSED: &str = "the run manifest refused a value of this backend";

#[cfg(test)]
mod tests {
    use std::error::Error as _;

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

    #[test]
    fn a_manifest_bug_keeps_its_cause_and_names_the_run_manifest() {
        let source = isoloom_manifest::EnvironmentName::parse("Not A Name").expect_err("refused");
        let e = Error::from(source);
        assert!(e.source().is_some());
        let reported = RenderError::from(e);
        assert_eq!(reported.to_string(), "docker: the run manifest refused a value of this backend");
        assert!(reported.source().is_some());
        assert_eq!(reported.refusal(), None);
    }
}
