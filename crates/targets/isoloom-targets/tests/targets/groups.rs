//! `common:` and `groups:`: shared machine fields fold into the machines before anything else
//! reads the spec; the machine's own value wins, then the most specific group, then `common`.

use isoloom_spec::{Target, groups, load, parse};
use isoloom_targets::{Lab, generate};

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

#[test]
fn shared_fields_fold_into_machines_with_the_right_precedence() {
    let lab = Lab::new(
        parse(&format!(
            "{BASE}common:\n  resources: {{ cpus: 1, memory_mb: 512 }}\n  os: debian@12\n  app: {{ steps: [{{ run: base.sh }}] }}\ngroups:\n  servers:\n    members: [srv*, db]\n    resources: {{ memory_mb: 4096 }}\n  big:\n    members: [servers]\n    resources: {{ cpus: 8 }}\n  pinned:\n    members: [db]\n    os: ubuntu@24.04\nmachines:\n  srv1: {{ networks: {{ lab: 11 }} }}\n  db: {{ networks: {{ lab: 12 }}, resources: {{ memory_mb: 8192 }}, app: {{ steps: [{{ run: p.sh }}] }} }}\n  user: {{ access: true, networks: {{ lab: 20 }} }}\n"
        ))
        .unwrap(),
    );
    assert_eq!(lab.problems(), vec![]);
    let spec = lab.spec();
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
    // `common:` reaches the access machine like any other: it is a Debian VM with the base step.
    assert_eq!(spec.machines["user"].vm.as_ref().unwrap().os, "debian-12");
    assert_eq!(spec.machines["user"].resources.unwrap().cpus, Some(1));
    assert_eq!(groups::members(&spec, "big"), ["srv1", "db"]);
    assert_eq!(groups::members(&spec, "pinned"), ["db"]);
}

#[test]
fn group_problems_name_the_field() {
    let p = |yaml: &str| -> Vec<String> {
        Lab::new(parse(yaml).unwrap())
            .problems()
            .iter()
            .map(isoloom_spec::syntax::Problem::line)
            .collect()
    };
    let m = "machines:\n  a: { networks: { lab: 5 }, app: { image: x } }\n";
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
    // A field a machine must own can't be shared: parsing says so.
    let err = parse(&format!("{BASE}common: {{ networks: {{ lab: 5 }} }}\n{m}")).unwrap_err().to_string();
    assert!(err.contains("`networks` is each machine's own"), "{err}");
}

#[test]
fn groups_become_inventory_groups_and_snapshot_entries() {
    let ad = Lab::new(load(&crate::testing::examples().join("corp-ad-basics")).unwrap());
    assert_eq!(ad.source().machines["dc01"].resources.unwrap().memory_mb, Some(4096));
    assert_eq!(groups::members(&ad.spec(), "domain"), ["dc01", "ws-01", "ws-02"]);
    let r = isoloom_targets::resolved::resolve(&ad);
    assert_eq!(
        isoloom_targets::resolved::lookup(&r, "groups.domain").unwrap(),
        &serde_json::json!(["dc01", "ws-01", "ws-02"])
    );
    // ansible-pair: the machines' os from common's; the group lands in the inventory.
    let pair = Lab::new(load(&crate::testing::examples().join("ansible-pair")).unwrap());
    assert_eq!(pair.spec().machines["web"].vm.as_ref().unwrap().os, "debian-12");
    assert_eq!(pair.source().machines["cache"].resources.unwrap().memory_mb, Some(1024));
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
