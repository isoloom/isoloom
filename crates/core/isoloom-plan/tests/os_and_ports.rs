//! A machine's `os: <family>@<version>` and its `ports`, as a spec is parsed, lowered and
//! validated.

use std::num::NonZeroU16;

use isoloom_plan::Lab;
use isoloom_spec::syntax::{self, Code, Kind, Problem, Problems};

/// A spec with one network and these lines under `machines:`.
fn spec(machines: &str) -> String {
    format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n{machines}")
}

/// The lab of `yaml`, or the problems that stop it from being read.
fn lab(yaml: &str) -> Result<Lab, Problems> {
    Lab::parse(yaml)
}

/// The problems the parser refuses `yaml` with.
fn refused(yaml: &str) -> Vec<Problem> {
    syntax::parse(yaml).err().unwrap_or_default()
}

/// The one problem the parser refuses `yaml` with, on one line.
fn refused_line(yaml: &str) -> String {
    let found = refused(yaml);
    assert_eq!(found.len(), 1, "{found:#?}");
    found.first().map(Problem::line).unwrap_or_default()
}

#[test]
fn ports_are_a_map_keyed_by_name() {
    let lab = lab(&spec(
        "  web: { networks: { lab: 5 }, ports: { site: { port: 80, protocol: http } }, app: { image: nginx } }\n",
    ))
    .unwrap();
    let (name, port) = lab.source().machines["web"].ports.first().unwrap();
    assert_eq!((name.as_str(), port.port.map(NonZeroU16::get)), ("site", Some(80)));
    assert_eq!(lab.problems(), []);
    let found = refused(&spec("  web: { networks: { lab: 5 }, services: [{ port: 80 }], app: { image: nginx } }\n"));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!((found[0].code, found[0].path.as_str()), (Code::RemovedField, "machines.web.services"));
    assert!(found[0].fix.as_deref().unwrap_or_default().contains("`ports: {"), "{found:#?}");
}

#[test]
fn a_machine_with_only_an_os_is_a_vm_or_a_container_of_it() {
    let lab = lab(&spec("  box: { os: debian, networks: { lab: 5 }, command: [sleep, infinity] }\n")).unwrap();
    let m = &lab.source().machines["box"];
    assert_eq!(m.os.map(|o| o.to_string()).as_deref(), Some("debian@12"), "the default version is filled in");
    assert_eq!(m.kind, Kind::Bare);
    let lowered = lab.spec();
    let lowered = &lowered.machines["box"];
    assert_eq!(lowered.vm.as_ref().map(|v| v.os.as_str()), Some("debian-12"));
    assert!(lowered.docker.is_some(), "the OS's own image, kept running");
    assert_eq!(lab.problems(), []);
}

#[test]
fn a_vm_runs_the_machines_os() {
    let lab = lab(&spec(
        "  dc: { os: windows-server@2022, networks: { lab: 5 }, app: { steps: [{ run: dc.ps1 }] } }\n  ws: { os: windows@11, networks: { lab: 6 }, app: { steps: [{ run: ws.ps1 }] } }\n",
    )).unwrap();
    let lowered = lab.spec();
    assert_eq!(lowered.machines["dc"].vm.as_ref().unwrap().os, "windows-server-2022");
    assert_eq!(lowered.machines["ws"].vm.as_ref().unwrap().os, "windows-11");
    assert_eq!(lab.problems(), []);
}

#[test]
fn the_machines_os_wins_over_a_shared_os() {
    let lab = lab(&format!(
        "{}common: {{ os: debian@13 }}\n",
        spec("  a: { os: ubuntu, networks: { lab: 5 }, app: { steps: [{ run: a.sh }] } }\n  b: { networks: { lab: 6 }, app: { steps: [{ run: b.sh }] } }\n")
    ))
    .unwrap();
    let lowered = lab.spec();
    assert_eq!(lowered.machines["a"].vm.as_ref().unwrap().os, "ubuntu-24.04");
    assert_eq!(lowered.machines["b"].vm.as_ref().unwrap().os, "debian-13");
}

#[test]
fn unknown_families_and_versions_are_refused_with_the_known_ones() {
    let e = refused_line(&spec("  a: { os: gentoo, networks: { lab: 5 }, app: { steps: [{ run: a.sh }] } }\n"));
    assert!(
        e.starts_with("machines.a.os: unknown OS `gentoo`; write `<family>@<version>` with one of: debian (11, 12, 13)"),
        "{e}"
    );
    assert_eq!(
        refused_line(&spec("  a: { os: debian@10, networks: { lab: 5 }, app: { steps: [{ run: a.sh }] } }\n")),
        "machines.a.os: debian has no version `10`; use one of: 11, 12, 13 (`debian` alone is debian@12)"
    );
}

#[test]
fn image_table_names_are_not_spellings() {
    for machine in [
        "  a: { os: debian-12, networks: { lab: 5 }, app: { steps: [{ run: a.sh }] } }\n",
        "  a: { os: windows-server-2019, networks: { lab: 5 }, app: { steps: [{ run: a.ps1 }] } }\n",
    ] {
        let e = refused_line(&spec(machine));
        assert!(e.starts_with("machines.a.os: unknown OS `"), "{e}");
    }
    let e = refused_line(&format!(
        "{}common: {{ os: ubuntu-24.04 }}\n",
        spec("  b: { networks: { lab: 6 }, app: { steps: [{ run: b.sh }] } }\n")
    ));
    assert!(e.starts_with("machines.b.os: unknown OS `ubuntu-24.04`"), "{e}");
}

#[test]
fn a_container_keeps_its_os_without_becoming_a_vm() {
    let lab = lab(&spec("  web: { os: alpine, networks: { lab: 5 }, app: { image: nginx:alpine } }\n")).unwrap();
    let lowered = lab.spec();
    assert!(lowered.machines["web"].vm.is_none());
    assert_eq!(lowered.machines["web"].os.as_deref(), Some("alpine@3.19"));
    assert_eq!(lab.problems(), []);
}
