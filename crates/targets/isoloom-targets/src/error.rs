//! Why a target's files, a message or a report can't be made: one error for the crate, with
//! why a target's files weren't generated ([`GenerateError`], also what `generate_all` lists as
//! skipped) and the backends' own errors kept whole as the cause.

use isoloom_spec::model::Target;

/// What stops a target's files, the message or a report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The target refuses the lab: it isn't possible for it, has no generator, or its backend
    /// can't run something the lab asks for.
    #[error(transparent)]
    Refused(#[from] GenerateError),
    /// A `{{ path }}` of the message points at nothing in the resolved snapshot.
    #[error("message: `{{{{ {path} }}}}` points at nothing in the snapshot (see `isoloom inspect`){hint}")]
    Placeholder {
        /// The placeholder's dotted path.
        path: String,
        /// What the author may have meant (a machine's `services` that is now `ports`), else empty.
        hint: &'static str,
    },
    /// The message has a `{{` without its `}}`.
    #[error("message: a `{{{{` without its `}}}}`")]
    Unclosed,
    /// No report has this name.
    #[error("no report named `{name}`; one of: {known}")]
    UnknownReport {
        /// The name asked for.
        name: String,
        /// The report names, comma separated.
        known: String,
    },
    /// A file the generated output copies from the project can't be read.
    #[error("a project file the output copies can't be read")]
    ProjectCopies(#[source] isoloom_kubernetes::Error),
}

/// What the crate returns: a value, or why it can't be made.
pub type Result<T, E = Error> = std::result::Result<T, E>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_error_names_the_placeholder_as_written() {
        let e = Error::Placeholder {
            path: "machines.web.ports.0".to_owned(),
            hint: "",
        };
        assert_eq!(
            e.to_string(),
            "message: `{{ machines.web.ports.0 }}` points at nothing in the snapshot (see `isoloom inspect`)"
        );
        assert_eq!(Error::Unclosed.to_string(), "message: a `{{` without its `}}`");
    }

    #[test]
    fn a_refusal_reads_as_the_generator_wrote_it() {
        let e = Error::from(GenerateError::NoGenerator(Target::Docker));
        assert_eq!(e.to_string(), "docker: no generator yet");
    }
}
