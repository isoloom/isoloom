//! The format against the paper exercise (cyber-project doc/architecture/LAB-RANGE-FORMAT.md):
//! the three example labs parse, validate, and get exactly the targets its matrix predicts.

use std::path::Path;

use isoloom_core::{Target, derive, effective, load, parse, totals, validate};

fn example(name: &str) -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).expect("example parses")
}

fn ids(targets: Vec<Target>) -> Vec<&'static str> {
    targets.into_iter().map(Target::id).collect()
}

const ALL: [&str; 7] = ["docker", "hosted", "cloud-docker", "vagrant", "proxmox", "cloud-vm", "ludus"];
const VM_ONLY: [&str; 4] = ["vagrant", "proxmox", "cloud-vm", "ludus"];

#[test]
fn supplier_portal_runs_everywhere() {
    let r = example("supplier-portal-api");
    assert_eq!(validate(&r), vec![]);
    assert_eq!(ids(derive(&r)), ALL);
}

#[test]
fn pivot_dmz_runs_everywhere_and_the_access_machine_needs_no_container() {
    let r = example("pivot-dmz");
    assert_eq!(validate(&r), vec![]);
    assert_eq!(ids(derive(&r)), ALL);
}

#[test]
fn ad_range_is_vm_only_even_with_one_container_ready_machine() {
    let r = example("corp-ad-basics");
    assert_eq!(validate(&r), vec![]);
    // web01 could be a container, but dc01 and ws01 can't: all-or-nothing.
    assert_eq!(ids(derive(&r)), VM_ONLY);
    let t = totals(&r);
    assert_eq!((t.machines, t.memory_mb), (4, 4096 + 4096 + 1024 + 1024));
}

fn problems(yaml: &str) -> Vec<String> {
    validate(&parse(yaml).expect("parses")).into_iter().map(|p| p.to_string()).collect()
}

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

#[test]
fn asking_for_an_impossible_target_names_the_machines() {
    let p = problems(&format!(
        "{BASE}targets: [docker]\nmachines:\n  dc: {{ networks: {{ lab: 5 }}, services: [{{ port: 389 }}], vm: {{ os: windows-server-2022, provision: [p.ps1] }} }}\n"
    ));
    assert_eq!(p, ["targets[0]: `docker` needs a `docker:` implementation on every machine; missing on: dc"]);
}

#[test]
fn narrowing_targets_keeps_only_those() {
    let r = parse(&format!("{BASE}targets: [docker, vagrant]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, docker: {{ image: nginx }}, vm: {{ os: debian-12, provision: [p.sh] }} }}\n")).unwrap();
    assert_eq!(ids(effective(&r)), ["docker", "vagrant"]);
}

#[test]
fn addresses_and_networks_are_checked() {
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  a: { cidr: 192.168.1.0/24 }\n  b: { cidr: 10.1.0.0/24 }\n  c: { cidr: 10.1.0.0/25 }\nmachines:\n  x: { networks: { b: 1 }, docker: { image: nginx } }\n  y: { networks: { b: 7, nope: 3 }, docker: { image: nginx } }\n  z: { networks: { b: 7 }, docker: { image: nginx } }\n",
    );
    assert!(p.iter().any(|m| m.starts_with("networks.a.cidr: use a block inside 10.0.0.0/8")), "{p:?}");
    assert!(p.iter().any(|m| m == "networks.c.cidr: overlaps network `b`"), "{p:?}");
    assert!(p.iter().any(|m| m.starts_with("machines.x.networks.b: 1 isn't a usable address")), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.y.networks.nope: no network named `nope`"), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.z.networks.b: address .7 is already used by `y`"), "{p:?}");
}

#[test]
fn dependencies_must_exist_have_services_and_not_loop() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, services: [{{ port: 1 }}], depends_on: [b], docker: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, services: [{{ port: 2 }}], depends_on: [a], docker: {{ image: x }} }}\n  c: {{ networks: {{ lab: 7 }}, depends_on: [ghost], docker: {{ image: x }} }}\n"
    ));
    assert!(p.iter().any(|m| m == "machines: depends_on forms a cycle: a -> b -> a"), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.c.depends_on[0]: no machine named `ghost`"), "{p:?}");
}

#[test]
fn inputs_reach_only_machines_that_declare_them() {
    let p = problems(&format!(
        "{BASE}inputs: [TOKEN]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, inputs: [SECRET], docker: {{ image: x }} }}\n"
    ));
    assert_eq!(p, ["machines.a.inputs[0]: `SECRET` isn't declared in the spec's `inputs`"]);
}

#[test]
fn unknown_fields_are_rejected() {
    let e = parse(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, flag: x, docker: {{ image: x }} }}\n"
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("unknown field `flag`"), "{e}");
}

#[test]
fn a_machine_needs_an_implementation_unless_it_is_the_access_machine() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }} }}\n  u: {{ access: true, networks: {{ lab: 6 }} }}\n"
    ));
    assert!(
        p.contains(&"machines.a: give the machine at least one implementation: `docker:` and/or `vm:`".to_string()),
        "{p:?}"
    );
    assert!(!p.iter().any(|m| m.starts_with("machines.u")), "{p:?}");
}
