//! Where the checkouts are: this Isoloom one, and the consumers next to the main checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The Isoloom checkout this xtask was built from (its repository root).
pub(crate) fn root() -> PathBuf {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));
    xtask.parent().map_or_else(|| xtask.to_path_buf(), Path::to_path_buf)
}

/// The folder holding the main Isoloom checkout, so a worktree finds the same consumers: the
/// parent of the repository's common git folder, else the parent of `root`.
pub(crate) fn code_dir(root: &Path) -> PathBuf {
    let common = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
    let main = common.as_deref().and_then(Path::parent).unwrap_or(root);
    main.parent().map_or_else(|| main.to_path_buf(), Path::to_path_buf)
}

/// A consumer's checkout: `$<var>` when set, else `<code dir>/<name>`.
pub(crate) fn consumer(var: &str, code_dir: &Path, name: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(|| code_dir.join(name), PathBuf::from)
}

/// A path as it is on disk (symlinks resolved), or as given when it doesn't exist. Windows'
/// verbatim prefix (`\\?\`) is dropped: the tools take the plain form.
pub(crate) fn physical(path: &Path) -> PathBuf {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = real.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(plain) => PathBuf::from(plain),
        None => real,
    }
}

/// A path as text with `/` separators, as the tools on every host read it.
pub(crate) fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Removes a file, or a folder with what it holds, when it is there.
pub(crate) fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
