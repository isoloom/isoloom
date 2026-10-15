//! Generating every example for every target stays under a fixed time budget (PLAN 3d), so a
//! change that makes generation much slower fails here, not in a user's hands.
//!
//! The budget is the time measured when it was set (2026-10-15: about 2 s for the 28 examples,
//! a debug build on the Intel Mac with other builds running) times about four, so a loaded
//! machine or a slower host still passes and only a real regression fails. Raise it only with
//! the reason in the commit.

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The most every example on every target may take, parsing included.
const BUDGET: Duration = Duration::from_secs(8);

#[test]
fn every_example_on_every_target_generates_within_the_budget() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let start = Instant::now();
    let mut files = 0;
    for dir in &dirs {
        let spec = isoloom_spec::load(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        files += isoloom_targets::generate_all(&isoloom_targets::Lab::new(spec)).0.len();
    }
    let took = start.elapsed();
    assert!(files > 0, "no example generated anything");
    assert!(took < BUDGET, "{} examples took {took:?} to generate, over the {BUDGET:?} budget", dirs.len());
}
