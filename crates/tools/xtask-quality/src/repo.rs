//! Small path helpers the quality checks need. The repository root is passed in by each
//! repository's `xtask`; the rest of the checkout layout lives in each `xtask`'s own `repo`.

use std::path::Path;

/// A path with forward slashes, so comparisons and reports read the same on every platform.
#[must_use]
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Removes a file, or a folder with what it holds, when it is there.
///
/// # Errors
///
/// Fails when the path exists but can't be removed (it is absent is not an error).
pub fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
