//! Where the checkout is, and its build folder.

use std::path::{Path, PathBuf};

// `slashed` and `remove` are shared with Isothrum through the ratchet crate.
pub(crate) use xtask_quality::repo::{remove, slashed};

/// The Isoloom checkout this xtask was built from (its repository root).
pub(crate) fn root() -> PathBuf {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));
    xtask.parent().map_or_else(|| xtask.to_path_buf(), Path::to_path_buf)
}

/// The build folder: `CARGO_TARGET_DIR`, else the workspace's `target`.
pub(crate) fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
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
