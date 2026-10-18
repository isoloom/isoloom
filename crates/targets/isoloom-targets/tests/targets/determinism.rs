//! Generation is deterministic: every example, rendered twice in one process for every target
//! it runs on, gives the same files byte for byte (no hash-map order, no clock, no randomness).

use std::path::PathBuf;

/// Every file of every target the lab allows, as (path, contents).
fn render(lab: &isoloom_targets::Lab) -> Vec<(String, String)> {
    let (files, _skipped) = isoloom_targets::generate_all(lab);
    files.into_iter().map(|f| (f.path, f.contents)).collect()
}

#[test]
fn every_example_renders_the_same_twice() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let mut rendered = 0;
    for dir in dirs {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())));
        let first = render(&lab);
        rendered += first.len();
        let second = render(&lab);
        assert_eq!(first.len(), second.len(), "{}: a different number of files", dir.display());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.0, b.0, "{}: files in a different order", dir.display());
            assert!(a.1 == b.1, "{}: {} differs between two renders", dir.display(), a.0);
        }
    }
    assert!(rendered > 0, "no example generated anything");
}
