//! Every example generates on every target it allows: the fast, untimed counterpart of the
//! generation-time budget, which `cargo xtask bench` holds (`generate/every-example`). And every
//! VM name a Vagrantfile gives is ASCII, which Vagrant's messages need.

use isoloom_targets::{generate, refusal, resolved};

use crate::testing::{Outcome, examples};

/// Every example's folder, in order.
fn example_dirs() -> Outcome<Vec<std::path::PathBuf>> {
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|entry| entry.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    dirs.sort();
    Ok(dirs)
}

#[test]
fn every_example_generates_on_every_target_it_allows() -> Outcome {
    let dirs = example_dirs()?;
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

/// The assignments of a Vagrantfile that name a VM under a provider.
const VM_NAMES: [&str; 4] = ["v.name =", "v.vmx[\"displayName\"] =", "v.guest_name =", "v.vmname ="];

#[test]
fn every_vm_name_a_vagrantfile_gives_is_ascii() -> Outcome {
    let mut named = 0;
    for dir in &example_dirs()? {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(dir)?);
        for target in lab.possible().into_iter().filter(|t| refusal(&lab, *t).is_none()) {
            let files = generate(&lab, target).map_err(|e| format!("{}: {}: {e}", dir.display(), target.id()))?;
            for f in files.iter().filter(|f| f.path.ends_with("/Vagrantfile")) {
                for line in f.contents.lines().filter(|l| VM_NAMES.iter().any(|n| l.trim_start().starts_with(n))) {
                    assert!(line.is_ascii(), "{}: {line}", f.path);
                    named += 1;
                }
            }
        }
    }
    assert!(named > 0, "the examples name VMs");
    Ok(())
}
