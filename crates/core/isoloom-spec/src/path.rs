//! A project's relative paths: a file or folder under the project's folder, `/`-separated on
//! every host, never absolute and never climbing out with `..` (what a generated file or a
//! drawing may name in the project).

use std::fmt;
use std::str::FromStr;

use crate::project_path::{NotPlain, plain};

/// Why a path isn't a project-relative one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// It isn't plain ([`crate::project_path::plain`], the one rule of a path in the project).
    #[error("`{path}` isn't a path inside the project folder: {why}")]
    NotPlain {
        /// The path as written.
        path: String,
        /// The rule it breaks.
        why: NotPlain,
    },
    /// A part of it is `.`.
    #[error("`{0}` has a `.` part: name the file by its one spelling")]
    CurrentDir(String),
}

/// A file or folder relative to the project's folder: a plain path
/// ([`crate::project_path::plain`]: relative, `/`-separated, names only), and with no `.` part
/// either. A `RelPath` names one entry (a file a drawing or a starter writes, a generated file's
/// path) by one spelling, so `a` and `./a` can't be two names of one file; a path the spec gives
/// to read from takes `.` (the project folder itself).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

impl RelPath {
    /// The path, when it follows the rules.
    ///
    /// # Errors
    ///
    /// [`PathError::NotPlain`] with the rule of [`plain`] it breaks, else
    /// [`PathError::CurrentDir`] for a `.` part.
    pub fn parse(path: &str) -> Result<RelPath, PathError> {
        plain(path).map_err(|why| PathError::NotPlain {
            path: path.escape_default().to_string(),
            why,
        })?;
        if path.split('/').any(|part| part == ".") {
            return Err(PathError::CurrentDir(path.to_owned()));
        }
        Ok(RelPath(path.to_owned()))
    }

    /// The path as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RelPath {
    type Err = PathError;

    fn from_str(path: &str) -> Result<RelPath, PathError> {
        RelPath::parse(path)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_path_is_plain_and_names_its_entry_by_one_spelling() {
        assert_eq!(RelPath::parse("a/b.sh").map(|p| p.to_string()), Ok("a/b.sh".to_owned()));
        let not_plain = |p: &str| match RelPath::parse(p) {
            Err(PathError::NotPlain { why, .. }) => Some(why),
            _ => None,
        };
        assert_eq!(not_plain(""), Some(NotPlain::Empty));
        assert_eq!(not_plain("a\\b"), Some(NotPlain::Backslash));
        assert_eq!(not_plain("/etc"), Some(NotPlain::Absolute));
        assert_eq!(not_plain("C:x"), Some(NotPlain::Drive));
        assert_eq!(not_plain("a/../../b"), Some(NotPlain::Parent));
        assert_eq!(not_plain("a\0"), Some(NotPlain::Control));
        assert!(matches!(RelPath::parse("./a"), Err(PathError::CurrentDir(_))));
        assert!(matches!(RelPath::parse("a/./b"), Err(PathError::CurrentDir(_))));
        assert!(RelPath::parse("a..b/c").is_ok(), "only a whole `..` part goes up");
    }
}
