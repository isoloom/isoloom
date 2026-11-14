use std::collections::BTreeSet;

use isoloom_manifest::contract::OUTPUT_DIR;

use super::*;
use crate::tiers::Tier;

/// The matrix, worked out once per test.
fn built() -> Matrix {
    matrix().unwrap()
}

/// The environments a row is decided on, by target: its plain one and `external`'s.
fn labs_of(p: &Probe) -> Vec<(Target, Lab)> {
    let probes = probe::probes().unwrap();
    Target::ALL
        .into_iter()
        .filter(|t| *t != Target::CloudServices && (p.only.is_empty() || p.only.contains(t)))
        .filter_map(|t| {
            let lab = match t {
                Target::External => lab(&probes.base, &p.patch, p.hosts),
                _ => lab(&probes.base, &p.patch, false),
            };
            lab.ok().map(|l| (t, l))
        })
        .collect()
}

/// The features `lab` asks of `target`.
fn asked(lab: &Lab, target: Target) -> BTreeSet<Feature> {
    lab.plan(&lab.for_target(target), target, None)
        .expect("the spec plans")
        .requirements
        .iter()
        .map(isoloom_plan::Requirement::feature)
        .collect()
}

#[test]
fn every_row_is_a_valid_spec_wherever_it_applies() {
    let m = built();
    for row in &m.rows {
        for (column, cell) in m.columns.iter().zip(&row.cells) {
            if let Cell::Refused(r) = cell {
                assert_ne!(r.code, Code::Invalid, "{} on {}: {}", row.name(), column.id(), r.reason);
            }
        }
        assert!(row.cells.iter().any(|c| *c != Cell::NotApplicable), "{} applies nowhere", row.name());
    }
}

#[test]
fn the_lab_every_row_changes_is_generated_everywhere_but_where_a_network_is_refused() {
    let probes = probe::probes().unwrap();
    let base = Probe {
        patch: Value::Mapping(serde_yaml_ng::Mapping::new()),
        ..probes
            .rows
            .iter()
            .find(|r| r.field == "machines.*.os" && r.value.as_deref() == Some("debian@12"))
            .unwrap()
            .clone()
    };
    let row = row(&probes, &base, &columns()).unwrap();
    for (column, cell) in columns().iter().zip(&row.cells) {
        let expected = match column.id().as_str() {
            "cloud-services" => Cell::NotApplicable,
            _ => Cell::Supported,
        };
        assert_eq!(*cell, expected, "{}", column.id());
    }
}

/// The features lowering rules out before any backend is asked: `hardware.devices` gives a
/// machine no VM form, `nested_virtualization` no container form, so no environment asks a backend for a
/// VM with the host's devices or a container nesting VMs. Their rows still show what each target
/// does with the field. When lowering lets one through, this test says so.
const RULED_OUT: [Feature; 2] = [Feature::VmDevices, Feature::ContainerNestedVirtualization];

#[test]
fn every_feature_of_the_plan_is_asked_for_by_a_row_that_asks_for_it() {
    let probes = probe::probes().unwrap();
    let mut asked_somewhere = BTreeSet::new();
    for p in probes.rows.iter().filter(|p| !p.features.is_empty()) {
        let found: BTreeSet<Feature> = labs_of(p).iter().flat_map(|(t, lab)| asked(lab, *t)).collect();
        assert!(p.features.iter().any(|f| found.contains(f)), "{}: none of {:?} asked for", p.field, p.features);
        asked_somewhere.extend(p.features.iter().copied().filter(|f| found.contains(f)));
    }
    let listed: BTreeSet<Feature> = probes.rows.iter().flat_map(|p| p.features.iter().copied()).collect();
    let missing: Vec<&str> = Feature::ALL.iter().filter(|f| !listed.contains(f)).map(|f| f.id()).collect();
    assert!(missing.is_empty(), "features no row asks for: {missing:?}");
    let unasked: BTreeSet<Feature> = listed.iter().copied().filter(|f| !asked_somewhere.contains(f)).collect();
    assert_eq!(unasked, BTreeSet::from(RULED_OUT), "features listed by a row whose lab never asks for them");
}

#[test]
fn each_row_is_named_once() {
    let m = built();
    let names: BTreeSet<(String, &str)> = m.rows.iter().map(|r| (r.name(), r.about.as_str())).collect();
    assert_eq!(names.len(), m.rows.len());
}

#[test]
fn a_column_carries_its_entries_tiers() {
    let m = built();
    let column = |id: &str| m.columns.iter().find(|c| c.id() == id).copied().unwrap();
    assert_eq!(m.column_tiers(&column("vagrant/libvirt")), [Tier::One]);
    assert_eq!(m.column_tiers(&column("cloud-docker")), [Tier::Two]);
}

#[test]
fn the_text_the_markdown_and_the_json_hold_every_row_and_column() {
    let m = built();
    let text = m.text();
    let md = m.markdown();
    let json = m.json();
    for row in &m.rows {
        assert!(text.contains(&format!("\n{}: {}\n", row.name(), row.about)), "{}", row.name());
        assert!(md.contains(&format!("| `{}` | {} |", row.name(), row.about)), "{}", row.name());
    }
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), m.rows.len());
    assert!(rows.iter().all(|r| r["cells"].as_object().unwrap().len() == m.columns.len()));
    assert_eq!(json["columns"].as_array().unwrap().len(), m.columns.len());
    assert_eq!(json["tiers"].as_array().unwrap().len(), m.tiers.len());
    let grid = md
        .lines()
        .filter(|l| l.starts_with("| `") && l.contains(" | ✓") || l.contains(" | ✗") || l.contains(" | –"))
        .count();
    assert!(grid >= m.rows.len(), "{grid} grid lines");
}

#[test]
fn a_refused_cell_says_which_step_refused_and_why() {
    let m = built();
    let column = |id: &str| m.columns.iter().find(|c| c.id() == id).copied().unwrap();
    let row = |field: &str| m.rows.iter().find(|row| row.field == field).unwrap();
    let Some(Cell::Refused(lacking)) = m.cell(row("machines.*.aliases"), &column("kubernetes")) else {
        panic!("aliases on kubernetes")
    };
    assert_eq!(lacking.code, Code::Capability(Feature::MachineAliases));
    let Some(Cell::Refused(lowering)) = m.cell(row("machines.*.app.image"), &column("vagrant/virtualbox")) else {
        panic!("an image on vagrant")
    };
    assert_eq!((lowering.code, lowering.field.as_deref()), (Code::Lowering, Some("machines.m.app.image")));
    assert!(
        m.text().contains("✗ vagrant/*, proxmox, esxi, cloud-vm/*: lowering: an image's filesystem"),
        "a target's every column named once"
    );
}

/// Where a target that takes an `internet: false` network closes the internet to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sealed {
    /// By the run, once the machines are set up: its run manifest's `lifecycle.seal`, out of a
    /// guest's reach (a cloud's firewalls, a server's router).
    ByTheRun,
    /// By what the target makes, from the start (`why` says how): its run manifest, if it has
    /// one, has no seal of its own.
    AsMade(&'static str),
}

/// How `target` seals an `internet: false` network; `None` where the row means nothing (no
/// machines of its own). A target added to the spec must say, so a ✓ can't go unexplained.
fn sealed_by(target: Target) -> Option<Sealed> {
    Some(match target {
        Target::Proxmox | Target::Esxi | Target::CloudVm => Sealed::ByTheRun,
        Target::Docker | Target::Hosted | Target::CloudDocker => Sealed::AsMade("an internal Compose network: the containers have no route out"),
        Target::Kubernetes => Sealed::AsMade("an egress network policy on the offline pods"),
        Target::Vagrant | Target::External => Sealed::AsMade("each machine's own egress step, at the end of its set-up"),
        Target::CloudServices => return None,
    })
}

/// The matrix's ✓ for `networks.*.internet = false` is what the generators write: every column
/// that takes it seals the network, and a target sealed by the run (a cloud, Proxmox, ESXi)
/// writes the seal into its run manifest. ESXi's ✓ once stood for a router that never sealed.
#[test]
fn every_target_the_matrix_says_takes_an_offline_network_seals_it() {
    let m = built();
    let probes = probe::probes().unwrap();
    let p = probes
        .rows
        .iter()
        .find(|p| p.field == "networks.*.internet" && p.value.as_deref() == Some("false"))
        .unwrap();
    let row = m.rows.iter().find(|r| r.name() == "networks.*.internet = false").unwrap();
    let labs = labs_of(p);
    let mut by_the_run = BTreeSet::new();
    for (column, cell) in m.columns.iter().zip(&row.cells) {
        if *cell != Cell::Supported {
            continue;
        }
        let how = sealed_by(column.target).unwrap_or_else(|| panic!("{}: takes the row, but says nothing of a seal", column.id()));
        let lab = &labs.iter().find(|(t, _)| *t == column.target).unwrap().1;
        let files = crate::generate(lab, column.target).unwrap();
        // The column's own run manifests: a cloud's in its folder, every other target's in its own.
        let folder = match column.variant.filter(|_| column.target == Target::CloudVm) {
            Some(cloud) => format!(".isoloom/{}/{cloud}/", column.target.id()),
            None => format!(".isoloom/{}/", column.target.id()),
        };
        let manifests: Vec<isoloom_manifest::Manifest> = files
            .iter()
            .filter(|f| f.path.starts_with(&folder) && f.path.ends_with("/run.json"))
            .map(|f| isoloom_manifest::parse(&f.contents).unwrap())
            .collect();
        match how {
            Sealed::ByTheRun => {
                assert!(!manifests.is_empty(), "{}: sealed by the run, but it writes no run manifest", column.id());
                for man in &manifests {
                    let seal = man.lifecycle.seal.as_ref();
                    assert!(seal.is_some(), "{}: ✓ in the matrix, but its run manifest has no seal", column.id());
                    assert_eq!(seal.map(|s| s.param.as_str()), Some("isoloom_sealed"), "{}", column.id());
                }
                by_the_run.insert(column.target);
            }
            Sealed::AsMade(why) => {
                for man in &manifests {
                    assert!(man.lifecycle.seal.is_none(), "{}: said sealed by {why}, yet its run seals", column.id());
                }
            }
        }
    }
    assert_eq!(by_the_run, BTreeSet::from([Target::Proxmox, Target::Esxi, Target::CloudVm]));
}

/// A column that generates nothing has no place in the matrix nor a tier: a provider or cloud
/// every row refuses isn't a way to run an environment. `vagrant/utm` was such a column (UTM
/// gives a VM no private network, and every machine is on one) and sat at tier 2.
#[test]
fn every_column_generates_some_row() {
    let m = built();
    for (i, column) in m.columns.iter().enumerate() {
        let generated = m.rows.iter().filter(|row| row.cells[i] == Cell::Supported).count();
        assert!(generated > 0, "{}: no row is generated there, yet it is a column", column.id());
    }
    let tiers = crate::tiers::tiers().unwrap();
    for entry in tiers.iter().filter(|e| e.target == Target::Vagrant) {
        assert!(m.columns.iter().any(|c| c.id() == entry.id()), "{}: a tier entry with no column", entry.id());
    }
}

/// A row about a step shows what each target does with the step, the container targets
/// included: its environment runs the step as the machine's only recipe (no `app.image` the container
/// targets would run instead, skipping the step), and each column's cell is what the generator
/// says of that environment. The playbook and reboot rows once kept the image, so the container columns
/// showed ✓ for steps they refuse.
#[test]
fn every_step_row_is_decided_on_the_step_itself() {
    let m = built();
    let probes = probe::probes().unwrap();
    let containers = [Target::Docker, Target::Hosted, Target::CloudDocker, Target::Kubernetes];
    for p in probes.rows.iter().filter(|p| p.field.starts_with("machines.*.app.steps[]")) {
        let row = m.rows.iter().find(|r| r.field == p.field && r.value == p.value && r.about == p.about).unwrap();
        for (target, lab) in labs_of(p) {
            let steps_only = matches!(&lab.source().machines["m"].kind, isoloom_spec::syntax::Kind::App(a) if a.container.is_none() && !a.steps.is_empty());
            assert!(steps_only, "{}: `m` has a recipe beside its steps", row.name());
            if !containers.contains(&target) {
                continue;
            }
            let cell = m
                .columns
                .iter()
                .zip(&row.cells)
                .find(|(c, _)| c.target == target)
                .map(|(_, cell)| cell)
                .unwrap();
            match (crate::refusal(&lab, target).unwrap(), cell) {
                (None, Cell::Supported) => {}
                (Some(why), Cell::Refused(r)) => assert!(why.ends_with(&r.reason), "{} on {}: {why} / {}", row.name(), target.id(), r.reason),
                (theirs, cell) => panic!("{} on {}: the generator says {theirs:?}, the matrix {cell:?}", row.name(), target.id()),
            }
        }
    }
}

/// The matrix's ✓ for a check is what the generators write: every column that takes a row about
/// `checks[]` writes the check (its run manifest entry, its runner line) in its own files, the
/// folder of its cloud or provider where it has one (`cloud-vm/aws/`), else its target's.
/// `external` once had a ✓ for script and playbook checks, and for probes with no host to run
/// from, while its runners left them out; and the test once looked at one column per target, in
/// any file of the target, so one cloud dropping a check passed on another's.
#[test]
fn every_column_the_matrix_says_takes_a_check_writes_it() {
    let m = built();
    let probes = probe::probes().unwrap();
    let mut looked = 0;
    for p in probes.rows.iter().filter(|p| p.field.starts_with("checks[]")) {
        let row = m.rows.iter().find(|r| r.field == p.field && r.value == p.value && r.about == p.about).unwrap();
        let labs = labs_of(p);
        for (column, cell) in m.columns.iter().zip(&row.cells) {
            if *cell != Cell::Supported {
                continue;
            }
            let lab = &labs.iter().find(|(t, _)| *t == column.target).unwrap().1;
            let spec = lab.for_target(column.target);
            let written: Vec<String> = isoloom_plan::checks::plan(&spec).into_iter().filter(|c| !c.derived).map(|c| c.name).collect();
            assert!(!written.is_empty(), "{}: the row's lab has no check", row.name());
            let files = crate::generate(lab, column.target).unwrap();
            // The column's own files: its cloud's or provider's folder, when the target writes
            // one, else the target's files but the resolved snapshot, which lists every check.
            let variant_dir = column.variant.map(|v| format!("{OUTPUT_DIR}/{}/{v}/", column.target.id()));
            let folder = variant_dir.filter(|dir| files.iter().any(|f| f.path.starts_with(dir.as_str())));
            let own = |f: &&isoloom_render::GeneratedFile| match &folder {
                Some(dir) => f.path.starts_with(dir.as_str()),
                None => f.path != crate::resolved::PATH,
            };
            for name in &written {
                assert!(
                    files.iter().filter(own).any(|f| f.contents.contains(name.as_str())),
                    "{} on {}: ✓ in the matrix, but none of its own files ({folder:?}) holds the check `{name}`",
                    row.name(),
                    column.id()
                );
            }
            looked += 1;
        }
    }
    assert!(looked > m.columns.len(), "{looked} columns looked at");
}

/// Rows a target marks generated without its files changing, each with why that is right:
/// `(row, target, why)`.
const NO_FILE_CHANGE: &[(&str, &str, &str)] = &[
    (
        "machines.*.app.image",
        "docker",
        "the base's container form already runs the image; the patch only removes steps it doesn't use",
    ),
    (
        "machines.*.app.image",
        "hosted",
        "the base's container form already runs the image; the patch only removes steps it doesn't use",
    ),
    (
        "machines.*.app.image",
        "cloud-docker",
        "the base's container form already runs the image; the patch only removes steps it doesn't use",
    ),
    (
        "machines.*.app.image",
        "kubernetes",
        "the base's container form already runs the image; the patch only removes steps it doesn't use",
    ),
    (
        "machines.*.app.steps",
        "vagrant",
        "the base's VM form already runs the steps; the patch only removes an image it doesn't use",
    ),
    (
        "machines.*.app.steps",
        "proxmox",
        "the base's VM form already runs the steps; the patch only removes an image it doesn't use",
    ),
    (
        "machines.*.app.steps",
        "esxi",
        "the base's VM form already runs the steps; the patch only removes an image it doesn't use",
    ),
    (
        "machines.*.app.steps",
        "cloud-vm",
        "the base's VM form already runs the steps; the patch only removes an image it doesn't use",
    ),
    ("machines.*.privileged", "vagrant", "a VM has kernel-level access already"),
    ("machines.*.privileged", "proxmox", "a VM has kernel-level access already"),
    ("machines.*.privileged", "esxi", "a VM has kernel-level access already"),
    ("machines.*.privileged", "cloud-vm", "a VM has kernel-level access already"),
    (
        "networks.*.ipv6",
        "kubernetes",
        "the base has no port, so no Service whose IP families the IPv6 side sets",
    ),
    (
        "networks.*.ipv6.addresses = auto",
        "kubernetes",
        "the base has no port, so no Service whose IP families the IPv6 side sets",
    ),
];

/// Rows a target marks generated without its files changing, and no reason found yet: each one
/// is either a field the backend drops (to refuse or render) or a reason to move above. Listed so
/// the test holds every other row; none may be added without one of those two outcomes.
const SUSPECTED_DROPS: &[(&str, &str, &str)] = &[
    ("machines.*.dns", "vagrant", "unverified: Proxmox and ESXi write the resolver, these don't"),
    ("machines.*.dns", "cloud-vm", "unverified: Proxmox and ESXi write the resolver, these don't"),
    (
        "machines.*.hardware.nested_virtualization",
        "proxmox",
        "unverified: the VM's CPU type is the same either way",
    ),
];

/// The target's own files `lab` generates on `target`, or None when it doesn't generate: the
/// resolved snapshot and `.gitignore` that go with every target are left out, since the
/// snapshot names every field whatever the target does with it.
fn files_of(lab: &Lab, target: Target) -> Option<Vec<(String, String)>> {
    let shared = [crate::resolved::file(lab).path, crate::gitignore::file().path];
    let files = crate::generate(lab, target).ok()?;
    Some(files.into_iter().filter(|f| !shared.contains(&f.path)).map(|f| (f.path, f.contents)).collect())
}

#[test]
fn a_row_generated_on_a_target_changes_that_targets_files() {
    // A ✓ that leaves the files as they were is a field the backend dropped: `tools:` read as
    // generated on Kubernetes, Proxmox, ESXi and cloud-vm while their files had no toolbox.
    let probes = probe::probes().unwrap();
    let nothing = Value::Mapping(serde_yaml_ng::Mapping::new());
    let mut dropped = Vec::new();
    let mut compared = 0;
    let mut excused_seen = BTreeSet::new();
    for p in probes.rows.iter().filter(|p| !p.cloud) {
        let name = match &p.value {
            Some(v) => format!("{} = {v}", p.field),
            None => p.field.clone(),
        };
        for (target, patched) in labs_of(p) {
            if target == Target::External || decide(&patched, target, None).ok() != Some(Decision::Generated) {
                continue;
            }
            let Ok(base) = lab(&probes.base, &nothing, false) else { continue };
            let (Some(before), Some(after)) = (files_of(&base, target), files_of(&patched, target)) else {
                continue;
            };
            compared += 1;
            let excused = NO_FILE_CHANGE
                .iter()
                .chain(SUSPECTED_DROPS)
                .any(|(row, t, _)| *row == name && *t == target.id());
            if before == after {
                if excused {
                    excused_seen.insert(format!("{name} on {}", target.id()));
                } else {
                    dropped.push(format!("{name} on {}", target.id()));
                }
            }
        }
    }
    assert!(dropped.is_empty(), "generated, files unchanged:\n{}", dropped.join("\n"));
    let stale: Vec<String> = NO_FILE_CHANGE
        .iter()
        .chain(SUSPECTED_DROPS)
        .map(|(row, t, _)| format!("{row} on {t}"))
        .filter(|e| !excused_seen.contains(e))
        .collect();
    assert!(
        stale.is_empty(),
        "listed as unchanged, but its files change now (take it off the list):\n{}",
        stale.join("\n")
    );
    // Most rows are generated on most targets: a handful compared means the loop skipped them.
    assert!(compared > 300, "only {compared} rows compared");
}
