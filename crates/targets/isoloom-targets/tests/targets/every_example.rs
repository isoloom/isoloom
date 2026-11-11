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

/// The examples' reference (`docs/reference/examples.md`): each example's row and the targets its
/// last column lists.
fn documented_targets() -> Outcome<Vec<(String, Vec<String>)>> {
    let page = std::fs::read_to_string(examples().join("../docs/reference/examples.md"))?;
    Ok(page
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.strip_prefix("| ")?.trim_end_matches('|').split(" | ").map(str::trim).collect();
            let name = cells.first()?.strip_prefix('`')?.strip_suffix('`')?;
            let targets = cells.last()?.split(", ").map(str::to_owned).collect();
            Some((name.to_owned(), targets))
        })
        .collect())
}

#[test]
fn the_examples_reference_lists_every_example_with_the_targets_it_allows() -> Outcome {
    let documented = documented_targets()?;
    let mut names = Vec::new();
    for dir in example_dirs()? {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(&dir)?);
        let name = dir.file_name().and_then(|n| n.to_str()).ok_or("an example's folder")?.to_owned();
        let allowed: Vec<String> = lab
            .possible()
            .into_iter()
            .filter(|t| refusal(&lab, *t).is_none())
            .map(|t| t.id().to_owned())
            .collect();
        let row = documented.iter().find(|(n, _)| *n == name).map(|(_, t)| t);
        assert_eq!(row, Some(&allowed), "docs/reference/examples.md's row for `{name}` against `isoloom targets`");
        names.push(name);
    }
    let extra: Vec<&String> = documented.iter().map(|(n, _)| n).filter(|n| !names.contains(n)).collect();
    assert!(extra.is_empty(), "docs/reference/examples.md lists examples that don't exist: {extra:?}");
    Ok(())
}

/// Whether `text` holds `lab` as one part of a name written with dashes or underscores
/// (`cache_lab_back`, `-cache-lab-back"`).
fn names_lab(text: &str) -> bool {
    text.match_indices("lab").any(|(at, _)| {
        let before = text.get(..at).and_then(|t| t.chars().next_back());
        let after = text.get(at + 3..).and_then(|t| t.chars().next());
        matches!(before, Some('-' | '_')) && matches!(after, Some('-' | '_' | '"'))
    })
}

/// No generated name says "environment" for the environment: an example that names nothing `lab` gets
/// none (GCP's sealed machines' firewall rule was `<machine>_lab_<network>`).
#[test]
fn no_generated_name_says_lab_unless_the_spec_does() -> Outcome {
    let mut looked = 0;
    for dir in &example_dirs()? {
        let spec = std::fs::read_to_string(dir.join("isoloom.yml"))?;
        if spec.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "lab") {
            continue;
        }
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(dir)?);
        for target in lab.possible().into_iter().filter(|t| refusal(&lab, *t).is_none()) {
            let files = generate(&lab, target).map_err(|e| format!("{}: {}: {e}", dir.display(), target.id()))?;
            for f in &files {
                assert!(!names_lab(&f.contents), "{}: {}", dir.display(), f.path);
            }
            looked += 1;
        }
    }
    assert!(looked > 0);
    Ok(())
}
