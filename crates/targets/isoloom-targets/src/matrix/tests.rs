use std::collections::BTreeSet;

use super::*;
use crate::tiers::Tier;

/// The matrix, worked out once per test.
fn built() -> Matrix {
    matrix().unwrap()
}

/// The labs a row is decided on, by target: its plain one, `hybrid`'s and `external`'s.
fn labs_of(p: &Probe) -> Vec<(Target, Lab)> {
    let probes = probe::probes().unwrap();
    Target::ALL
        .into_iter()
        .filter(|t| *t != Target::CloudServices && (p.only.is_empty() || p.only.contains(t)))
        .filter_map(|t| {
            let lab = match t {
                Target::Hybrid => lab(&probes.hybrid, &p.patch, false),
                Target::External => lab(&probes.base, &p.patch, p.hosts),
                _ => lab(&probes.base, &p.patch, false),
            };
            lab.ok().map(|l| (t, l))
        })
        .collect()
}

/// The features `lab` asks of `target`.
fn asked(lab: &Lab, target: Target) -> BTreeSet<Feature> {
    Environment::build(&lab.for_target(target), target, None)
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
    let row = row(&probes, &base, &columns());
    for (column, cell) in columns().iter().zip(&row.cells) {
        let expected = match column.id().as_str() {
            "cloud-services" => Cell::NotApplicable,
            // vagrant_utm gives a VM no private network: every machine is on one.
            "vagrant/utm" => continue,
            _ => Cell::Supported,
        };
        assert_eq!(*cell, expected, "{}", column.id());
    }
}

/// The features lowering rules out before any backend is asked: `hardware.devices` gives a
/// machine no VM form, `nested_virtualization` no container form, so no lab asks a backend for a
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
    assert_eq!(m.column_tiers(&column("docker-vm")), [Tier::Two]);
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
