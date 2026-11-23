//! Isoloom writes native files only: no target writes a run manifest (`run.json`).

use isoloom_spec::model::Target;
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, examples};

#[test]
fn no_target_writes_a_run_manifest() -> Outcome {
    let mut labs = Vec::new();
    for entry in std::fs::read_dir(examples())? {
        let dir = entry?.path();
        if dir.join("isoloom.yml").is_file() {
            labs.push(Lab::new(isoloom_spec::load(&dir)?));
        }
    }
    for target in Target::ALL {
        for files in labs.iter().filter_map(|lab| generate(lab, target).ok()) {
            assert!(!files.iter().any(|f| f.path.ends_with("run.json")), "`{}` writes a run manifest", target.id());
        }
    }
    Ok(())
}
