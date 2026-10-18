//! Every example generates on every target it allows: the fast, untimed counterpart of the
//! generation-time budget, which `cargo xtask bench` holds (`generate/every-example`).

use isoloom_targets::{generate, refusal, resolved};

use crate::testing::{Outcome, examples};

#[test]
fn every_example_generates_on_every_target_it_allows() -> Outcome {
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|entry| entry.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    dirs.sort();
    let mut generated = 0;
    for dir in &dirs {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(dir)?);
        // A target that refuses the lab says why instead (tested with each refusal).
        for target in lab.possible().into_iter().filter(|t| refusal(&lab, *t).is_none()) {
            let files = generate(&lab, target).map_err(|e| format!("{}: {}: {e}", dir.display(), target.id()))?;
            // The resolved snapshot comes with every target; the target's own files must too.
            assert!(
                files.iter().any(|f| f.path != resolved::PATH),
                "{}: {} wrote only the snapshot",
                dir.display(),
                target.id()
            );
            generated += 1;
        }
    }
    assert!(generated >= dirs.len(), "{generated} generations for {} examples", dirs.len());
    Ok(())
}
