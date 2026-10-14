//! A machine's `os: <family>@<version>` and its `ports` (named `services` before 1.0), as a spec
//! is parsed and validated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // a failing test is the report.

use isoloom_spec::{parse, validate, warnings};

/// A spec with one network and these lines under `machines:`.
fn spec(machines: &str) -> String {
    format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n{machines}")
}

fn problems(yaml: &str) -> Vec<String> {
    validate(&parse(yaml).unwrap_or_else(|e| panic!("{e}")))
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_old_services_key_is_read_with_a_warning() {
    let s = parse(&spec(
        "  web: { networks: { lab: 5 }, services: [{ port: 80, http: true }], docker: { image: nginx } }\n",
    ))
    .unwrap();
    assert_eq!(s.machines["web"].ports[0].port, 80);
    assert_eq!(validate(&s), vec![]);
    let w: Vec<String> = warnings(&s).iter().map(ToString::to_string).collect();
    assert_eq!(
        w,
        ["machines.web.services: `services` is now `ports`: rename the key (`services` is still read, with this warning, until a later release removes it)"]
    );
}

#[test]
fn the_old_key_reaches_clones_and_groups_like_the_new_one() {
    let s = parse(&spec(
        "  ws: { count: 2, networks: { lab: 5 }, services: [{ port: 22 }], docker: { image: x } }\n",
    ))
    .unwrap();
    assert_eq!(s.machines["ws-02"].ports[0].port, 22);
    assert_eq!(warnings(&s).len(), 1, "one warning for the machine as written");
}

#[test]
fn a_machine_setting_both_keys_is_refused() {
    let e = parse(&spec(
        "  web: { networks: { lab: 5 }, services: [{ port: 80 }], ports: [{ port: 80 }], docker: { image: x } }\n",
    ))
    .unwrap_err();
    assert_eq!(
        e.to_string(),
        "machines.web: sets both `ports` and `services`, its name before 1.0; keep `ports` only"
    );
}

#[test]
fn a_spec_with_ports_has_no_warning() {
    let s = parse(&spec("  web: { networks: { lab: 5 }, ports: [{ port: 80 }], docker: { image: x } }\n")).unwrap();
    assert_eq!(warnings(&s), []);
}

#[test]
fn a_machine_with_only_an_os_is_a_vm_of_it() {
    let s = parse(&spec("  box: { os: debian, networks: { lab: 5 }, command: [sleep, infinity] }\n")).unwrap();
    let m = &s.machines["box"];
    assert_eq!(m.os.as_deref(), Some("debian@12"), "the default version is filled in");
    assert_eq!(m.vm.as_ref().map(|v| v.os.as_str()), Some("debian-12"));
    assert!(m.docker.is_none());
    assert_eq!(validate(&s), vec![]);
}

#[test]
fn vm_os_defaults_to_the_machines_and_takes_the_new_syntax() {
    let s = parse(&spec(
        "  dc: { os: windows-server@2022, networks: { lab: 5 }, vm: { provision: [dc.ps1] } }\n  ws: { networks: { lab: 6 }, vm: { os: windows@11, provision: [ws.ps1] } }\n",
    ))
    .unwrap();
    assert_eq!(s.machines["dc"].vm.as_ref().unwrap().os, "windows-server-2022");
    assert_eq!(s.machines["ws"].vm.as_ref().unwrap().os, "windows-11");
    assert_eq!(validate(&s), vec![]);
}

#[test]
fn the_machines_os_wins_over_a_shared_vm_os() {
    let s = parse(&format!(
        "{}common: {{ vm: {{ os: debian-13 }} }}\n",
        spec("  a: { os: ubuntu, networks: { lab: 5 }, vm: { provision: [a.sh] } }\n  b: { networks: { lab: 6 }, vm: { provision: [b.sh] } }\n")
    ))
    .unwrap();
    assert_eq!(s.machines["a"].vm.as_ref().unwrap().os, "ubuntu-24.04");
    assert_eq!(s.machines["b"].vm.as_ref().unwrap().os, "debian-13");
}

#[test]
fn an_os_and_a_vm_os_that_disagree_are_refused_with_the_fix() {
    let p = problems(&spec(
        "  a: { os: debian@12, networks: { lab: 5 }, vm: { os: ubuntu-24.04, provision: [a.sh] } }\n",
    ));
    assert_eq!(
        p,
        [
            "machines.a.vm.os: `vm.os` is ubuntu@24.04 but the machine's `os` is debian@12: they must agree; leave `vm.os` out (it defaults to `os`) or write the same OS in both"
        ]
    );
    // The same OS in two spellings agrees.
    assert_eq!(
        problems(&spec("  a: { os: debian, networks: { lab: 5 }, vm: { os: debian-12, provision: [a.sh] } }\n")),
        Vec::<String>::new()
    );
}

#[test]
fn unknown_families_and_versions_are_refused_once_with_the_known_ones() {
    let p = problems(&spec("  a: { os: gentoo, networks: { lab: 5 }, vm: { provision: [a.sh] } }\n"));
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(
        p[0].starts_with("machines.a.os: unknown OS `gentoo`; write `<family>@<version>` with one of: debian (11, 12, 13)"),
        "{p:?}"
    );
    let p = problems(&spec("  a: { networks: { lab: 5 }, vm: { os: debian@10, provision: [a.sh] } }\n"));
    assert_eq!(
        p,
        ["machines.a.vm.os: debian has no version `10`; use one of: 11, 12, 13 (`debian` alone is debian@12)"]
    );
}

#[test]
fn a_container_keeps_its_os_without_becoming_a_vm() {
    let s = parse(&spec("  web: { os: alpine, networks: { lab: 5 }, docker: { image: nginx:alpine } }\n")).unwrap();
    assert!(s.machines["web"].vm.is_none());
    assert_eq!(s.machines["web"].os.as_deref(), Some("alpine@3.19"));
    assert_eq!(validate(&s), vec![]);
}
