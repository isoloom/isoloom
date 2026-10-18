//! The plan's one error: what a value given to the plan names that it can't use. The plan's own
//! findings about a spec are [`Problem`](isoloom_spec::syntax::Problem)s and refusals, not errors.

use isoloom_spec::model::Target;
use isoloom_spec::syntax::placeholder::Reference;

use crate::configure::Configure;

/// Why the plan can't use a value it was given.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A CM tool `--configure` doesn't know.
    #[error("`{0}` isn't a CM tool: use `shell`, `cloud-init` or `ansible`")]
    UnknownTool(String),
    /// A target that can't hand its machines to the CM tool asked for.
    #[error("{target}: `--configure {tool}`: {why}", target = target.id())]
    Unpaired {
        /// The tool asked for.
        tool: Configure,
        /// The target.
        target: Target,
        /// Why they don't pair.
        why: &'static str,
    },
    /// A template's placeholder that can't be read where it is.
    #[error("a template's placeholder: {0}")]
    Placeholder(#[source] isoloom_spec::Error),
    /// A template's placeholder that names nothing the lab has.
    #[error("`{{{{ {0} }}}}` names nothing this lab has")]
    Unfilled(Reference),
}

/// A result with the plan's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
