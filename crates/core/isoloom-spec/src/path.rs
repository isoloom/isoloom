//! A project's relative paths: a file or folder under the project's folder, `/`-separated on
//! every host, never absolute and never climbing out with `..` (what a generated file or a
//! drawing may name in the project).

use std::fmt;
use std::str::FromStr;

/// Why a path isn't a project-relative one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// An empty path.
    #[error("a path can't be empty")]
    Empty,
    /// A NUL character in it.
    #[error("a path holds no NUL character: {0}")]
    Nul(String),
    /// A backslash in it (paths are `/`-separated on every host).
    #[error("`{0}` has a backslash: paths are `/`-separated")]
    Backslash(String),
    /// A `/` or a drive first.
    #[error("`{0}` is absolute: a path is relative to the project's folder")]
    Absolute(String),
    /// A `..` segment.
    #[error("`{0}` climbs out of its folder with `..`")]
    Parent(String),
}

/// A file or folder relative to the project's folder: `/`-separated, never absolute, never
/// leaving its folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

impl RelPath {
    /// The path, when it follows the rules.
    ///
    /// # Errors
    ///
    /// A [`PathError`] naming the rule it breaks.
    pub fn parse(path: &str) -> Result<RelPath, PathError> {
        let owned = || path.to_owned();
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        if path.contains('\0') {
            return Err(PathError::Nul(path.escape_default().to_string()));
        }
        if path.contains('\\') {
            return Err(PathError::Backslash(owned()));
        }
        let drive = matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
        if path.starts_with('/') || drive {
            return Err(PathError::Absolute(owned()));
        }
        if path.split('/').any(|segment| segment == "..") {
            return Err(PathError::Parent(owned()));
        }
        Ok(RelPath(owned()))
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
    fn a_project_path_is_relative_slash_separated_and_stays_in_its_folder() {
        assert_eq!(RelPath::parse("a/b.sh").map(|p| p.to_string()), Ok("a/b.sh".to_owned()));
        assert_eq!(RelPath::parse(""), Err(PathError::Empty));
        assert!(matches!(RelPath::parse("a\\b"), Err(PathError::Backslash(_))));
        assert!(matches!(RelPath::parse("/etc"), Err(PathError::Absolute(_))));
        assert!(matches!(RelPath::parse("C:x"), Err(PathError::Absolute(_))));
        assert!(matches!(RelPath::parse("a/../../b"), Err(PathError::Parent(_))));
        assert!(matches!(RelPath::parse("a\0"), Err(PathError::Nul(_))));
        assert!(RelPath::parse("a..b/c").is_ok(), "only a whole `..` segment climbs");
    }
}
