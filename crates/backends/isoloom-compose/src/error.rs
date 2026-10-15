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
    #[error("{}: the Compose file couldn't be written as YAML", target.id())]
    Unwritable {
        /// The target whose file it is.
        target: Target,
        /// What the YAML writer reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// A value of the run manifest was refused by the manifest's own types: a bug of this
    /// backend, which builds every one of them.
    #[error("{}: the run manifest refused a value of this backend", Target::Docker.id())]
    Manifest {
        /// What the manifest's types refused.
        #[source]
        source: ManifestError,
    },
    /// A Compose file's text isn't YAML (reading its start plan back).
    #[error("the Compose file isn't YAML")]
    Unreadable {
        /// What the YAML reader reported.
        #[source]
        source: serde_yaml_ng::Error,
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
    /// The error as every backend reports it: the plan's refusal, or what the target doesn't
    /// support, its cause in its words.
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { target, refusal } => RenderError::Refused { target, refusal },
            Error::Unwritable { target, source } => unsupported(target, source.to_string()),
            Error::Manifest { source } => unsupported(Target::Docker, format!("the run manifest: {source}")),
            Error::Unreadable { source } => unsupported(Target::Docker, format!("the Compose file isn't YAML: {source}")),
        }
    }
}

fn unsupported(target: Target, what: String) -> RenderError {
    RenderError::Unsupported { target, what }
}

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
        let source = isoloom_manifest::LabName::parse("Not A Name").expect_err("refused");
        let e = Error::from(source);
        assert!(e.source().is_some());
        let reported = RenderError::from(e).to_string();
        assert!(reported.starts_with("docker: the run manifest: "), "{reported}");
    }
}
