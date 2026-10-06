//! `count:`: a machine written once becomes numbered clones with consecutive addresses, and
//! every place that named it means all of them.

use std::path::Path;

use isoloom_core::{checks, load, parse, validate};

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  office: { cidr: 10.9.1.0/24 }\n";

#[test]
fn a_count_becomes_numbered_clones_with_consecutive_addresses() {
    let spec = parse(&format!(
        "{BASE}groups:\n  fleet: {{ members: [ws], resources: {{ cpus: 2 }} }}\nmachines:\n  dc:\n    networks: {{ lab: 5 }}\n    services: [{{ port: 389 }}]\n    docker: {{ image: dc }}\n  ws:\n    count: 3\n    networks: {{ lab: 20, office: 50 }}\n    depends_on: [dc]\n    services: [{{ port: 22 }}]\n    docker: {{ image: ws, idle: true }}\n  user:\n    access: true\n    networks: {{ lab: 90 }}\n    depends_on: [ws]\nchecks:\n  - {{ name: the dc answers, from: ws, tcp: dc:389 }}\n"
    ))
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    let names: Vec<&String> = spec.machines.keys().collect();
    assert_eq!(names, ["dc", "ws-01", "ws-02", "ws-03", "user"]);
    assert_eq!(spec.machines["ws-01"].networks["lab"], 20);
    assert_eq!(spec.machines["ws-03"].networks["lab"], 22);
    assert_eq!(spec.machines["ws-03"].networks["office"], 52);
    assert!(spec.machines["ws-02"].count.is_none());
    // The group's field reached every clone; the base name in depends_on means all of them.
    assert_eq!(spec.machines["ws-02"].resources.unwrap().cpus, Some(2));
    assert_eq!(spec.machines["user"].depends_on, ["ws-01", "ws-02", "ws-03"]);
    assert_eq!(spec.clones["ws"], ["ws-01", "ws-02", "ws-03"]);
    // One check per clone, named after it.
    let own: Vec<String> = checks::plan(&spec).into_iter().filter(|c| !c.derived).map(|c| c.name).collect();
    assert_eq!(own, ["the dc answers (ws-01)", "the dc answers (ws-02)", "the dc answers (ws-03)"]);
}

#[test]
fn counts_that_make_no_sense_are_refused() {
    let m = |extra: &str| format!("{BASE}machines:\n  a:\n    networks: {{ lab: 250 }}\n    {extra}\n    docker: {{ image: x }}\n");
    assert!(parse(&m("count: 1")).unwrap_err().to_string().contains("2 to 99"));
    assert!(parse(&m("count: 100")).unwrap_err().to_string().contains("2 to 99"));
    assert!(parse(&m("count: 2\n    access: true")).unwrap_err().to_string().contains("access machine"));
    // Addresses that run past the block are caught by the usual address validation, on the clone.
    let spec = parse(&m("count: 5")).unwrap();
    let problems: Vec<String> = validate(&spec).into_iter().map(|p| p.at).collect();
    assert!(problems.iter().any(|p| p.starts_with("machines.a-05.networks")), "{problems:?}");
    let gw = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24, gateway: fw }\nmachines:\n  fw:\n    count: 2\n    networks: { lab: 1 }\n    docker: { image: x }\n",
    );
    assert!(gw.unwrap_err().to_string().contains("a gateway is one machine"));
}

#[test]
fn the_example_fleet_is_in_its_group_and_the_snapshot() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/corp-ad-basics")).unwrap();
    assert_eq!(spec.clones["ws"], ["ws-01", "ws-02"]);
    assert_eq!(isoloom_core::groups::members(&spec, "domain"), ["dc01", "ws-01", "ws-02"]);
    let r = isoloom_core::resolved::resolve(&spec);
    assert_eq!(isoloom_core::resolved::lookup(&r, "clones.ws").unwrap(), &serde_json::json!(["ws-01", "ws-02"]));
    assert_eq!(isoloom_core::resolved::lookup(&r, "machines.ws-02.addresses.corp").unwrap(), "10.40.20.22");
}
