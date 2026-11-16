//! Helpers the spec's integration tests share.

use std::error::Error;
use std::path::{Path, PathBuf};

/// A test's outcome: a failed step is its report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The workspace's root folder, where `docs/`, `examples/` and `schemas/` are.
pub(crate) fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
