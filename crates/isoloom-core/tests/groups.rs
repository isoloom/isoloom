//! `common:` and `groups:`: shared machine fields fold into the machines before anything else
//! reads the spec; the machine's own value wins, then the most specific group, then `common`.

use std::path::Path;

use isoloom_core::{Target, generate, groups, load, parse, validate};

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

#[test]
fn shared_fields_fold_into_machines_with_the_right_precedence() {
    let spec = parse(&format!(
        "{BASE}common:\n  resources: {{ cpus: 1, memory_mb: 512 }}\n  vm: {{ os: debian-12, provision: [base.sh] }}\ngroups:\n  servers:\n    members: [srv*, db]\n    resources: {{ memory_mb: 4096 }}\n  big:\n    members: [servers]\n    resources: {{ cpus: 8 }}\n  pinned:\n    members: [db]\n    vm: {{ os: ubuntu-24.04 }}\nmachines:\n  srv1: {{ networks: {{ lab: 11 }}, vm: {{}} }}\n  db: {{ networks: {{ lab: 12 }}, resources: {{ memory_mb: 8192 }}, vm: {{ provision: [p.sh] }} }}\n  user: {{ access: true, networks: {{ lab: 20 }} }}\n"
    ))
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    // srv1: in `servers`, hence in `big` too: big's cpus, servers' memory, common's os and steps.
    let srv = &spec.machines["srv1"];
    assert_eq!(srv.resources.unwrap().cpus, Some(8));
    assert_eq!(srv.resources.unwrap().memory_mb, Some(4096));
    assert_eq!(srv.vm.as_ref().unwrap().os, "debian-12");
    assert_eq!(srv.vm.as_ref().unwrap().provision, vec!["base.sh"]);
    // db: its own memory wins; `big` holds `servers`, so the member group is the more specific one
    // and `pinned` is declared later: cpus from big, os from pinned, provision its own.
    let db = &spec.machines["db"];
    assert_eq!(db.resources.unwrap().memory_mb, Some(8192));
    assert_eq!(db.resources.unwrap().cpus, Some(8));
    assert_eq!(db.vm.as_ref().unwrap().os, "ubuntu-24.04");
    assert_eq!(db.vm.as_ref().unwrap().provision, vec!["p.sh"]);
    // The access machine declares no `vm:`, so common's `vm:` doesn't make it one.
    assert!(spec.machines["user"].vm.is_none());
    assert_eq!(spec.machines["user"].resources.unwrap().cpus, Some(1));
    assert_eq!(groups::members(&spec, "big"), ["srv1", "db"]);
    assert_eq!(groups::members(&spec, "pinned"), ["db"]);
}

#[test]
fn group_problems_name_the_field() {
    let p = |yaml: &str| -> Vec<String> { validate(&parse(yaml).unwrap()).into_iter().map(|p| p.to_string()).collect() };
    let m = "machines:\n  a: { networks: { lab: 5 }, docker: { image: x } }\n";
    assert_eq!(
        p(&format!("{BASE}groups:\n  linux: {{ members: [a] }}\n{m}")),
        ["groups.linux: Isoloom fills this group itself; choose another name"]
    );
    assert_eq!(
        p(&format!("{BASE}groups:\n  g: {{ members: [nope] }}\n{m}")),
        ["groups.g.members[0]: `nope` names no machine or group"]
    );
    assert_eq!(
        p(&format!("{BASE}groups:\n  a: {{ members: [a] }}\n{m}")),
        ["groups.a: `a` is already a machine's name"]
    );
    assert!(
        p(&format!("{BASE}groups:\n  g: {{ members: [h] }}\n  h: {{ members: [g] }}\n{m}"))
            .iter()
            .any(|e| e.contains("loop"))
    );
    // A field a machine must own can't be shared: the schema says so at parse time.
    let err = parse(&format!("{BASE}common: {{ networks: {{ lab: 5 }} }}\n{m}")).unwrap_err().to_string();
    assert!(err.contains("unknown field `networks`"), "{err}");
}

#[test]
fn groups_become_inventory_groups_and_snapshot_entries() {
    let ad = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/corp-ad-basics")).unwrap();
    assert_eq!(ad.machines["dc01"].resources.unwrap().memory_mb, Some(4096));
    assert_eq!(groups::members(&ad, "domain"), ["dc01", "ws-01", "ws-02"]);
    let r = isoloom_core::resolved::resolve(&ad);
    assert_eq!(
        isoloom_core::resolved::lookup(&r, "groups.domain").unwrap(),
        &serde_json::json!(["dc01", "ws-01", "ws-02"])
    );
    // ansible-pair: `vm: {}` completed by common's os; the group lands in the inventory.
    let pair = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ansible-pair")).unwrap();
    assert_eq!(pair.machines["web"].vm.as_ref().unwrap().os, "debian-12");
    assert_eq!(pair.machines["cache"].resources.unwrap().memory_mb, Some(1024));
    let vf = generate(&pair, Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents;
    let after = vf.split("[pair]").nth(1).expect("the pair group in the inventory");
    let (w, c) = (after.find("web").unwrap_or(usize::MAX), after.find("cache").unwrap_or(usize::MAX));
    assert!(w < c && c < 200, "{after}");
}
