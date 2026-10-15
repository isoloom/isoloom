//! Why a written value isn't one the spec accepts: what `FromStr` of the spec's own types says.

use crate::os::OsError;

/// Why a value isn't one the spec accepts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    /// A name that breaks its rule.
    #[error("`{value}` isn't a valid {what}: {rule}")]
    Name {
        /// What it names (`port name`).
        what: &'static str,
        /// The value written.
        value: String,
        /// The rule, as an author fixes the value.
        rule: &'static str,
    },
    /// An OS no family and version name.
    #[error(transparent)]
    Os(#[from] OsError),
    /// A file mode that isn't octal digits up to `0777`.
    #[error("`{0}` isn't a file mode: octal digits up to 0777, like \"0444\"")]
    Mode(String),
}
