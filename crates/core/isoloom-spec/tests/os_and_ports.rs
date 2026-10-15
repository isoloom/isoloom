//! A machine's `os: <family>@<version>` and its `ports`, as a spec is parsed and validated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // a failing test is the report.

use isoloom_spec::{parse, validate};

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

/// The error loading `yaml` gives.
fn refused(yaml: &str) -> String {
    parse(yaml).map_or_else(|e| e.to_string(), |_| panic!("parsed: {yaml}"))
}

#[test]
fn the_ports_key_is_the_only_one() {
    let s = parse(&spec(
        "  web: { networks: { lab: 5 }, ports: [{ port: 80, http: true }], docker: { image: nginx } }\n",
    ))
    .unwrap();
    assert_eq!(s.machines["web"].ports[0].port, 80);
    assert_eq!(validate(&s), vec![]);
    let e = refused(&spec("  web: { networks: { lab: 5 }, services: [{ port: 80 }], docker: { image: nginx } }\n"));
    assert!(e.starts_with("unknown field `services`, expected one of"), "{e}");
    assert!(e.contains("`ports`"), "{e}");
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
        "{}common: {{ vm: {{ os: debian@13 }} }}\n",
        spec("  a: { os: ubuntu, networks: { lab: 5 }, vm: { provision: [a.sh] } }\n  b: { networks: { lab: 6 }, vm: { provision: [b.sh] } }\n")
    ))
    .unwrap();
    assert_eq!(s.machines["a"].vm.as_ref().unwrap().os, "ubuntu-24.04");
    assert_eq!(s.machines["b"].vm.as_ref().unwrap().os, "debian-13");
}

#[test]
fn an_os_and_a_vm_os_that_disagree_are_refused_with_the_fix() {
    let p = problems(&spec(
        "  a: { os: debian@12, networks: { lab: 5 }, vm: { os: ubuntu@24.04, provision: [a.sh] } }\n",
    ));
    assert_eq!(
        p,
        [
            "machines.a.vm.os: `vm.os` is ubuntu@24.04 but the machine's `os` is debian@12: they must agree; leave `vm.os` out (it defaults to `os`) or write the same OS in both"
        ]
    );
    // The family alone and its default version agree.
    assert_eq!(
        problems(&spec("  a: { os: debian, networks: { lab: 5 }, vm: { os: debian@12, provision: [a.sh] } }\n")),
        Vec::<String>::new()
    );
}

#[test]
fn unknown_families_and_versions_are_refused_with_the_known_ones() {
    let e = refused(&spec("  a: { os: gentoo, networks: { lab: 5 }, vm: { provision: [a.sh] } }\n"));
    assert!(
        e.starts_with("machines.a.os: unknown OS `gentoo`; write `<family>@<version>` with one of: debian (11, 12, 13)"),
        "{e}"
    );
    assert_eq!(
        refused(&spec("  a: { networks: { lab: 5 }, vm: { os: debian@10, provision: [a.sh] } }\n")),
        "machines.a.vm.os: debian has no version `10`; use one of: 11, 12, 13 (`debian` alone is debian@12)"
    );
}

#[test]
fn image_table_names_are_not_spellings() {
    for (at, machine) in [
        ("machines.a.os", "  a: { os: debian-12, networks: { lab: 5 }, vm: { provision: [a.sh] } }\n"),
        (
            "machines.a.vm.os",
            "  a: { networks: { lab: 5 }, vm: { os: windows-server-2019, provision: [a.ps1] } }\n",
        ),
    ] {
        let e = refused(&spec(machine));
        assert!(e.starts_with(&format!("{at}: unknown OS `")), "{e}");
    }
    let e = refused(&format!(
        "{}common: {{ vm: {{ os: ubuntu-24.04 }} }}\n",
        spec("  b: { networks: { lab: 6 }, vm: { provision: [b.sh] } }\n")
    ));
    assert!(e.starts_with("machines.b.vm.os: unknown OS `ubuntu-24.04`"), "{e}");
}

#[test]
fn a_container_keeps_its_os_without_becoming_a_vm() {
    let s = parse(&spec("  web: { os: alpine, networks: { lab: 5 }, docker: { image: nginx:alpine } }\n")).unwrap();
    assert!(s.machines["web"].vm.is_none());
    assert_eq!(s.machines["web"].os.as_deref(), Some("alpine@3.19"));
    assert_eq!(validate(&s), vec![]);
}
