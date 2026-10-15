//! The `external` target: machines that already exist, described with their SSH endpoints.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{Target, derive, load, parse, validate};
use isoloom_targets::generate;

fn example() -> isoloom_spec::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/existing-hosts")).unwrap()
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
    assert_eq!(isoloom_spec::targets::missing(&spec, isoloom_spec::Shape::External), ["b"]);
}

#[test]
fn existing_hosts_behind_a_gateway_machine_are_still_generated() {
    let spec = parse(
        "version: 1\nname: gw\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: fw }\n  wan: { cidr: 10.8.0.0/24 }\nmachines:\n  fw:\n    networks: { wan: 2, lan: 1 }\n    vm: { os: debian@12, provision: [fw.sh] }\n    external: { address: 192.168.1.2 }\n  pc:\n    networks: { lan: 10 }\n    vm: { os: debian@12, provision: [pc.sh] }\n    external: { address: 192.168.1.10 }\n",
    )
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    assert!(derive(&spec).contains(&Target::External));
    assert_eq!(isoloom_targets::refusal(&spec, Target::External), None);
    assert!(generate(&spec, Target::External).is_ok());
}
