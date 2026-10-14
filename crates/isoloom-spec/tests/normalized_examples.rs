//! Snapshots of every example's spec as the rest of Isoloom reads it: parsed, with clones,
//! groups and VLANs folded in. A change to parsing or normalization shows up here as a diff to
//! review (`cargo insta review`), not only as a changed generated file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::PathBuf;

fn examples() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs
}

#[test]
fn every_example_normalizes_as_snapshotted() {
    let dirs = examples();
    assert!(!dirs.is_empty(), "no examples found");
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let spec = isoloom_spec::load(&dir).unwrap_or_else(|e| panic!("{name}: {e}"));
        insta::assert_yaml_snapshot!(name, spec);
    }
}
