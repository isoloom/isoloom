//! A path the spec gives in the project folder (a build context, a file's source, a script): the
//! one rule every such path follows, written once. As written, it is plain: relative, made of
//! names only (no `..`, no root, no Windows drive or `\` separator, no NUL), the same on every
//! host. On disk, it leads to something inside the project folder once every symbolic link on
//! its way is followed: a project can hold links, and one leading out of it would hand the host's
//! own files to the environment.

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Why a path as written isn't a plain path of the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotPlain {
    /// It is empty.
    Empty,
    /// It holds a NUL or another control character.
    Control,
    /// It holds a `\`: the separator is `/` on every host.
    Backslash,
    /// It starts at a root (`/etc`).
    Absolute,
    /// It starts with a Windows drive (`C:`, `C:/x`).
    Drive,
    /// A part of it is `..`.
    Parent,
}

impl fmt::Display for NotPlain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NotPlain::Empty => "it is empty",
            NotPlain::Control => "it holds a control character",
            NotPlain::Backslash => "it holds a `\\`: separate folders with `/`",
            NotPlain::Absolute => "it starts at the root",
            NotPlain::Drive => "it starts with a drive",
            NotPlain::Parent => "it goes up a folder (`..`)",
        })
    }
}

/// Whether `path` is plain: relative to the project folder and made of names only (`.` parts are
/// allowed: `.` is the project folder itself).
///
/// # Errors
///
/// The first rule it breaks, in [`NotPlain`]'s order.
pub fn plain(path: &str) -> Result<(), NotPlain> {
    if path.is_empty() {
        return Err(NotPlain::Empty);
    }
    if path.chars().any(char::is_control) {
        return Err(NotPlain::Control);
    }
    if path.contains('\\') {
        return Err(NotPlain::Backslash);
    }
    if path.starts_with('/') {
        return Err(NotPlain::Absolute);
    }
    if matches!(path.as_bytes(), [d, b':', ..] if d.is_ascii_alphabetic()) {
        return Err(NotPlain::Drive);
    }
    // `/` is the only separator left: each part is a name, `.` or `..`.
    if path.split('/').any(|part| part == "..") {
        return Err(NotPlain::Parent);
    }
    debug_assert!(Path::new(path).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)));
    Ok(())
}

/// Where a path of the project leads on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Escape {
    /// It isn't plain as written.
    NotPlain(NotPlain),
    /// Nothing is there (a link to nothing included).
    Missing,
    /// It leads out of the project folder, through a symbolic link: where it leads.
    Outside(PathBuf),
}

impl fmt::Display for Escape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Escape::NotPlain(why) => write!(f, "it isn't a path inside the project folder: {why}"),
            Escape::Missing => f.write_str("it doesn't exist in the project folder"),
            Escape::Outside(_) => f.write_str("it leads out of the project folder through a symbolic link"),
        }
    }
}

impl std::error::Error for NotPlain {}

impl std::error::Error for Escape {}

impl Escape {
    /// What's wrong with `path` (the path as written), and what to do: a problem's message.
    #[must_use]
    pub fn about(&self, path: &str) -> String {
        match self {
            Escape::NotPlain(why) => format!("`{path}` must be a path inside the project folder: {why}"),
            Escape::Missing => format!("`{path}` doesn't exist in the project folder"),
            Escape::Outside(_) => {
                format!("`{path}` leads out of the project folder through a symbolic link: keep the file itself in the project")
            }
        }
    }
}

/// The real place of `path` in project folder `dir`, every link followed, when it is plain, there,
/// and inside `dir`'s own real place: the only path a reader of the project's files opens.
///
/// # Errors
///
/// [`Escape`]: why it isn't such a path.
pub fn inside(dir: &Path, path: &str) -> Result<PathBuf, Escape> {
    plain(path).map_err(Escape::NotPlain)?;
    let root = dir.canonicalize().map_err(|_| Escape::Missing)?;
    let real = dir.join(path).canonicalize().map_err(|_| Escape::Missing)?;
    if real.starts_with(&root) { Ok(real) } else { Err(Escape::Outside(real)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_names_under_the_project_folder() {
        for ok in ["a", "a/b.sh", "./a", ".", "site/", "v1..2/x", "a b/c"] {
            assert_eq!(plain(ok), Ok(()), "{ok}");
        }
        let refused = [
            ("", NotPlain::Empty),
            ("a\0b", NotPlain::Control),
            ("a\nb", NotPlain::Control),
            ("..\\..\\x", NotPlain::Backslash),
            ("a\\b", NotPlain::Backslash),
            ("/etc/passwd", NotPlain::Absolute),
            ("C:/x", NotPlain::Drive),
            ("C:x", NotPlain::Drive),
            ("..", NotPlain::Parent),
            ("a/../../x", NotPlain::Parent),
            ("a/..", NotPlain::Parent),
        ];
        for (path, why) in refused {
            assert_eq!(plain(path), Err(why), "{path:?}");
        }
    }

    #[test]
    fn a_path_is_inside_once_its_links_are_followed() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = crate::testing::folder("project-path-inside")?;
        let dir = scratch.path();
        std::fs::create_dir_all(dir.join("real"))?;
        std::fs::write(dir.join("real/file"), "")?;
        assert_eq!(inside(dir, "real/file")?, dir.canonicalize()?.join("real/file"));
        assert_eq!(inside(dir, "gone"), Err(Escape::Missing));
        assert_eq!(inside(dir, "../x"), Err(Escape::NotPlain(NotPlain::Parent)));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("real", dir.join("alias"))?;
            std::os::unix::fs::symlink("/etc", dir.join("ctx"))?;
            std::os::unix::fs::symlink("/nowhere/at/all", dir.join("dangling"))?;
            assert_eq!(inside(dir, "alias/file")?, dir.canonicalize()?.join("real/file"));
            assert!(matches!(inside(dir, "ctx"), Err(Escape::Outside(_))));
            assert!(matches!(inside(dir, "ctx/hostname"), Err(Escape::Outside(_) | Escape::Missing)));
            assert_eq!(inside(dir, "dangling"), Err(Escape::Missing));
        }
        Ok(())
    }
}
