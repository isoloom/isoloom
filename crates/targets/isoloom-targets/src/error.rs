//! Why a target's files, a message or a report can't be made: one error for the crate, with
//! why a target's files weren't generated ([`GenerateError`], also what `generate_all` lists as
//! skipped) and the backends' own errors kept whole as the cause.

use std::sync::Arc;

use isoloom_spec::model::Target;

/// What stops a target's files, the message or a report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A target's files weren't generated: the target refuses the lab, has no generator, or its
    /// backend failed.
    #[error(transparent)]
    Generate(#[from] GenerateError),
    /// The lab can't be made the instance asked for (`isoloom_spec::instance::apply`).
    #[error("instance {number}: {source}")]
    Instance {
        /// The instance number asked for.
        number: u8,
        /// Why it can't be made.
        #[source]
        source: isoloom_spec::Error,
    },
    /// A `{{ path }}` of the message is one the message can't have, or points at nothing in the
    /// resolved snapshot.
    #[error("message: `{{{{ {path} }}}}` {why}")]
    Placeholder {
        /// The placeholder's dotted path.
        path: String,
        /// Why it can't be filled.
        why: String,
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
    /// The tier list (`tiers.json`) isn't JSON of its shape.
    #[error("tiers.json can't be read as the tier list")]
    TiersJson(#[source] serde_json::Error),
    /// A row of the capability matrix (`probes.yml`) names what doesn't exist.
    #[error("the capability matrix's row `{row}`: {what}")]
    Probe {
        /// The row's field.
        row: String,
        /// What is wrong with it.
        what: String,
    },
    /// An entry of the tier list names what doesn't exist.
    #[error("tiers.json: `{entry}`: {what}")]
    Tiers {
        /// The entry (`vagrant/libvirt`).
        entry: String,
        /// What is wrong with it.
        what: String,
    },
}

/// What the crate returns: a value, or why it can't be made.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a target's files weren't generated: a refusal ([`GenerateError::is_refusal`]), the
/// target leaving the lab out as `isoloom targets` says, or a failure, which no lab should
/// cause.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
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
    #[error("{}: no generator", .0.id())]
    NoGenerator(Target),
    /// The backend failed, not the lab (a bug: a value the run manifest's types refused, a file
    /// its writer couldn't write), with the error it failed with.
    #[error("{}: {what}", target.id())]
    Internal {
        /// The target being generated.
        target: Target,
        /// What failed.
        what: String,
        /// The error it failed with.
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
}

impl GenerateError {
    /// Whether the target refuses the lab ([`GenerateError::NotPossible`],
    /// [`GenerateError::Unsupported`]): the one reason a target may be left out of `generate`
    /// and `check` without failing them. No generator, or a failed one, is never the lab's
    /// doing.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        matches!(self, GenerateError::NotPossible(_) | GenerateError::Unsupported { .. })
    }
}

impl From<isoloom_render::Error> for GenerateError {
    /// A backend's error as the target's: a refusal or a limit of its own, read the same way;
    /// a failure kept with its cause.
    fn from(e: isoloom_render::Error) -> GenerateError {
        let target = e.target();
        match e {
            isoloom_render::Error::Internal { what, source, .. } => GenerateError::Internal { target, what, source },
            refused => GenerateError::Unsupported {
                target,
                what: refused.refusal().unwrap_or_default(),
            },
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
            why: "points at nothing in the snapshot (see `isoloom inspect`)".to_owned(),
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
        assert_eq!(e.to_string(), "docker: no generator");
    }

    #[test]
    fn only_a_refusal_of_the_lab_is_a_refusal() {
        let failed = GenerateError::from(isoloom_render::Error::internal(
            Target::Vagrant,
            "the run manifest refused a value of this backend",
            std::io::Error::other("bad"),
        ));
        assert!(matches!(failed, GenerateError::Internal { target: Target::Vagrant, .. }));
        assert!(std::error::Error::source(&failed).is_some());
        let refused = GenerateError::from(isoloom_render::Error::Unsupported {
            target: Target::Proxmox,
            what: "machine `w`: no image".to_owned(),
        });
        assert_eq!(refused.to_string(), "proxmox: machine `w`: no image");
        assert!(refused.is_refusal() && GenerateError::NotPossible(Target::Docker).is_refusal());
        assert!(!failed.is_refusal() && !GenerateError::NoGenerator(Target::Docker).is_refusal());
    }
}
