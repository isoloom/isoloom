//! The examples, written in the authored format, through lowering: each is valid, gets the
//! targets its machines' forms allow, and lowers for the lab as snapshotted (a change to parsing,
//! normalization or lowering shows up here as a diff to review, not only as a changed generated
//! file).

use std::path::{Path, PathBuf};

use isoloom_plan::Lab;
use testing::ids;

pub mod testing;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples")
}

/// The examples' names, sorted.
fn names() -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root())? {
        let path = entry?.path();
        if path.is_dir()
            && let Some(name) = path.file_name()
        {
            out.push(name.to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

fn example(name: &str) -> Result<Lab, String> {
    isoloom_spec::load(&root().join(name)).map(Lab::new).map_err(|e| format!("{name}: {e}"))
}

const ALL: [&str; 9] = [
    "docker",
    "hosted",
    "docker-vm",
    "cloud-docker",
    "kubernetes",
    "vagrant",
    "proxmox",
    "esxi",
    "cloud-vm",
];

#[test]
fn every_example_is_valid() {
    let names = names().unwrap();
    assert!(!names.is_empty(), "no examples found");
    for name in names {
        assert_eq!(example(&name).unwrap().problems(), [], "{name}");
    }
}

#[test]
fn every_example_lowers_as_snapshotted() {
    for name in names().unwrap() {
        insta::assert_yaml_snapshot!(name.clone(), example(&name).unwrap().spec());
    }
}

#[test]
fn a_lab_with_both_forms_on_every_machine_runs_everywhere() {
    for name in ["supplier-portal-api", "pivot-dmz", "edge-firewall"] {
        assert_eq!(ids(example(name).unwrap().possible()), ALL, "{name}");
    }
}

#[test]
fn a_windows_domain_is_vm_only_even_with_one_container_ready_machine() {
    let lab = example("corp-ad-basics").unwrap();
    // web01 could be a container, but dc01 and ws01 can't: VMs, or hybrid with web01 a container.
    assert_eq!(ids(lab.possible()), ["hybrid", "vagrant", "proxmox", "esxi", "cloud-vm"]);
    let t = isoloom_spec::totals(&lab.spec());
    // dc01 and the two workstations (`count: 2`) at 4 GB, web01 and the access machine at 1 GB.
    assert_eq!((t.machines, t.memory_mb), (5, 4096 * 3 + 1024 * 2));
}
