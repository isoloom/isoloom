//! The catalogue's one error: why it doesn't load, or what a value given to it breaks. What it
//! finds wrong in a machine's `service:` and `with:` is a spec [`Problem`], placed at the field,
//! not an error.
//!
//! [`Problem`]: isoloom_spec::syntax::Problem

use std::fmt;

/// Why the catalogue can't load, or a value it can't take.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file isn't valid for its type.
    #[error("catalogue/{path}: {source}")]
    Yaml {
        /// The file, from the catalogue's folder.
        path: String,
        /// What the parser says.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// An entry's folder lacks a file every entry has.
    #[error("catalogue/{entry}: no {file}; every entry has entry.yml, lock.yml and contract.yml")]
    Missing {
        /// The entry's folder.
        entry: String,
        /// The missing file.
        file: &'static str,
    },
    /// Entries that break the catalogue's rules (the entry checks).
    #[error("the catalogue breaks its rules:\n{}", list(.0))]
    Invalid(Vec<Finding>),
    /// A name that breaks its rule.
    #[error("`{value}` isn't a valid {what}: {rule}")]
    Name {
        /// What it names.
        what: &'static str,
        /// The name.
        value: String,
        /// The rule, as an author fixes it.
        rule: &'static str,
    },
    /// A version that isn't one.
    #[error("`{0}` isn't a version: write parts of letters and digits joined by dots (`1.30.6`)")]
    Version(String),
}

/// A result with the catalogue's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// One rule an entry breaks: where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file and field (`nginx/entry.yml: recipe.install[0]`).
    pub at: String,
    /// What is wrong, and what to write instead.
    pub reason: String,
}

impl Finding {
    /// A finding at `at`.
    #[must_use]
    pub fn new(at: impl Into<String>, reason: impl Into<String>) -> Finding {
        Finding {
            at: at.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "catalogue/{}: {}", self.at, self.reason)
    }
}

/// The findings, one line each.
fn list(findings: &[Finding]) -> String {
    findings.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")
}
