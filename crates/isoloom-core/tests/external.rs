//! The `external` target: machines that already exist, described with their SSH endpoints.

use std::path::Path;

use isoloom_core::{Target, derive, generate, load, parse, validate};

fn example() -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/existing-hosts")).unwrap()
}

#[test]
fn a_spec_with_addresses_on_every_machine_gets_the_external_target() {
    let spec = example();
    assert_eq!(validate(&spec), vec![]);
    let ids: Vec<&str> = derive(&spec).into_iter().map(Target::id).collect();
    assert!(ids.contains(&"external") && ids.contains(&"vagrant"), "{ids:?}");
    let files = generate(&spec, Target::External).unwrap();
    let inv = &files.iter().find(|f| f.path == ".isoloom/external/inventory.ini").unwrap().contents;
    assert!(
        inv.contains("web ansible_host=192.168.1.20 ansible_user=florian ansible_ssh_private_key_file=~/.ssh/id_ed25519"),
        "{inv}"
    );
    let machines = &files.iter().find(|f| f.path == ".isoloom/external/machines.json").unwrap().contents;
    assert!(machines.contains("\"port\": 22"), "{machines}");
    // Derived checks run from each machine; probes only.
    let web = &files.iter().find(|f| f.path == ".isoloom/external/checks/web.sh").unwrap().contents;
    assert!(web.contains("_tcp '192.168.1.21' 6379"), "{web}");
    assert!(files.iter().any(|f| f.path == ".isoloom/resolved.json"));
}

#[test]
fn external_blocks_are_checked_and_optional() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n";
    let spec = parse(&format!(
        "{base}  a: {{ networks: {{ lab: 5 }}, docker: {{ image: x }}, external: {{ address: '' }} }}\n"
    ))
    .unwrap();
    assert!(validate(&spec)[0].at.ends_with("external.address"));
    let spec = parse(&format!(
        "{base}  a: {{ networks: {{ lab: 5 }}, docker: {{ image: x }}, external: {{ address: 10.0.0.5, port: 0 }} }}\n"
    ))
    .unwrap();
    assert!(validate(&spec).iter().any(|p| p.at.ends_with("external.port")));
    // One machine without an address: no external target, and it says so.
    let spec = parse(&format!("{base}  a: {{ networks: {{ lab: 5 }}, docker: {{ image: x }}, external: {{ address: 10.0.0.5 }} }}\n  b: {{ networks: {{ lab: 6 }}, docker: {{ image: x }} }}\n")).unwrap();
    assert!(!derive(&spec).contains(&Target::External));
    assert_eq!(isoloom_core::targets::missing(&spec, isoloom_core::Shape::External), ["b"]);
}
