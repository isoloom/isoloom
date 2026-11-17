//! The paas feature, which the binary and the other steps leave off: clippy over the workspace
//! with it on, then the tests of the crates it changes, so it can't rot unseen.

use std::path::Path;

use super::cargo;
use crate::Res;

/// The crates the feature changes.
const CRATES: [&str; 4] = ["isoloom-spec", "isoloom-plan", "isoloom-terraform", "isoloom-targets"];
/// The feature that turns it on in all of them.
const FEATURE: &str = "isoloom-targets/paas";

/// Clippy with the feature on, then the tests of [`CRATES`]: whether both passed.
pub(super) fn run(root: &Path) -> Res<bool> {
    let clippy = [
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--features",
        FEATURE,
        "--",
        "-D",
        "warnings",
    ];
    if !cargo(root, &clippy)? {
        return Ok(false);
    }
    let mut test = vec!["nextest", "run", "--locked", "--no-tests=fail", "--features", FEATURE];
    for c in CRATES {
        test.extend(["-p", c]);
    }
    cargo(root, &test)
}
