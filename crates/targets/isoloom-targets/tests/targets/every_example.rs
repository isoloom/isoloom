//! Every example generates on every target it allows, and what it generates holds the generator's
//! invariants: the fast, untimed counterpart of the generation-time budget, which `cargo xtask
//! bench` holds (`generate/every-example`).

use std::path::PathBuf;

use isoloom_spec::model::Target;
use isoloom_targets::{GeneratedFile, generate, refusal, resolved};

use crate::testing::{Outcome, examples};

/// One example generated for one target it allows.
struct Generation {
    /// The example's folder.
    dir: PathBuf,
    /// Its spec's text.
    spec: String,
    /// The target.
    target: Target,
    /// What the target wrote.
    files: Vec<GeneratedFile>,
}

/// Every example generated on every target it allows (a refusing target says why instead, tested
/// with each refusal).
fn generations() -> Outcome<Vec<Generation>> {
    let mut all = Vec::new();
    for dir in example_dirs()? {
        let spec = std::fs::read_to_string(dir.join("isoloom.yml"))?;
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(&dir)?);
        for target in lab.possible().into_iter().filter(|t| refusal(&lab, *t).is_none()) {
            let files = generate(&lab, target).map_err(|e| format!("{}: {}: {e}", dir.display(), target.id()))?;
            all.push(Generation {
                dir: dir.clone(),
                spec: spec.clone(),
                target,
                files,
            });
        }
    }
    Ok(all)
}

/// Every example's folder, in order.
fn example_dirs() -> Outcome<Vec<std::path::PathBuf>> {
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|entry| entry.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    dirs.sort();
    Ok(dirs)
}

/// The assignments of a Vagrantfile that name a VM under a provider.
const VM_NAMES: [&str; 4] = ["v.name =", "v.vmx[\"displayName\"] =", "v.guest_name =", "v.vmname ="];

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

/// Every example generates on every target it allows, and every generation holds the
/// generator's invariants, each problem said (one sweep: generating every example on every
/// target is the slow part, done once):
/// - a target writes its own files, not only the resolved snapshot;
/// - every VM name a Vagrantfile gives is ASCII, which Vagrant's messages need;
/// - no generated name says "lab" for the environment unless the spec does (GCP's sealed
///   machines' firewall rule was `<machine>_lab_<network>`).
#[test]
fn every_example_generates_what_the_generator_promises_on_every_target_it_allows() -> Outcome {
    let dirs = example_dirs()?;
    let all = generations()?;
    let mut problems = Vec::new();
    for dir in &dirs {
        if !all.iter().any(|g| &g.dir == dir) {
            problems.push(format!("{}: no target generates it", dir.display()));
        }
    }
    let (mut vm_names, mut lab_free) = (0, 0);
    for g in &all {
        let at = format!("{}: {}", g.dir.display(), g.target.id());
        if g.files.iter().all(|f| f.path == resolved::PATH) {
            problems.push(format!("{at}: wrote only the snapshot"));
        }
        for f in g.files.iter().filter(|f| f.path.ends_with("/Vagrantfile")) {
            for line in f.contents.lines().filter(|l| VM_NAMES.iter().any(|n| l.trim_start().starts_with(n))) {
                vm_names += 1;
                if !line.is_ascii() {
                    problems.push(format!("{at}: {}: a VM name that isn't ASCII: {line}", f.path));
                }
            }
        }
        if !g.spec.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "lab") {
            lab_free += 1;
            for f in g.files.iter().filter(|f| names_lab(&f.contents)) {
                problems.push(format!("{at}: {} names something `lab`", f.path));
            }
        }
    }
    assert!(vm_names > 0, "the examples name VMs");
    assert!(lab_free > 0, "an example names nothing `lab`");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}
