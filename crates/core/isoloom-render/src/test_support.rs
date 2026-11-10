//! What tests of the crates above this one build their cases from, behind the `test-support`
//! feature (a dev-dependency's): never part of the API a backend renders with.

use crate::run_manifest::guest::{GuestCommand, GuestCompose};

/// An office LAN split into VLAN 10 (offline) and VLAN 20 (online): `admin` and `nas` are on
/// both, `staff` on one, `ops` on both and a network of its own, `win` is a Windows VM on both.
/// The trunks' tests read it, a VM's and a container's alike.
pub const VLAN_OFFICE: &str = r"
version: 1
name: v
networks:
  office:
    cidr: 10.70.0.0/16
    internet: false
    vlans:
      10: { cidr: 10.70.10.0/24 }
      20: { cidr: 10.70.20.0/24, internet: true }
  servers: { cidr: 10.70.99.0/24 }
machines:
  admin: { os: debian@12, networks: { office.vlan10: 50, office.vlan20: 50 } }
  nas: { os: debian@12, networks: { office.vlan20: 60, office.vlan10: 60 } }
  staff: { os: debian@12, networks: { office.vlan10: 10 } }
  ops: { os: debian@12, networks: { office.vlan10: 70, office.vlan20: 70, servers: 70 } }
  win: { os: windows-server@2022, networks: { office.vlan10: 80, office.vlan20: 80 } }
";

/// A guest with nothing in it but `status`: no machines, slots, steps or checks.
#[must_use]
pub fn empty_guest(status: GuestCommand) -> GuestCompose {
    GuestCompose {
        machines: Vec::new(),
        networks: Vec::new(),
        slots: Vec::new(),
        params: Vec::new(),
        start: Vec::new(),
        ready: Vec::new(),
        check_setup: Vec::new(),
        check_teardown: Vec::new(),
        runners: Vec::new(),
        status,
    }
}

/// A POSIX `sh` to run a generated script with: the system's on Unix; on Windows the one on
/// `PATH` (Git's `bin` has one), else `None`, and the test that asked passes without running it,
/// as those needing Ruby or `tar` do. The scripts are for Linux guests: a Windows computer runs
/// them only when it has a `sh` to.
#[must_use]
pub fn posix_sh() -> Option<std::process::Command> {
    #[cfg(windows)]
    if std::process::Command::new("sh").args(["-c", "true"]).output().is_err() {
        return None;
    }
    Some(std::process::Command::new("sh"))
}

/// `path` as a script for `sh` takes it: with forward slashes, which Git's `sh` on Windows reads
/// as the folders they separate (it reads a backslash as an escape).
#[must_use]
pub fn sh_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A spec for inventory tests: a Linux VM on two networks with every kind of `host_vars`, a
/// Windows VM answering WinRM over HTTPS on Vagrant, a container, inventory groups, and both
/// VMs as existing hosts on `external`.
pub const INVENTORY_SPEC: &str = r"
version: 1
name: inv
networks:
  lab: { cidr: 10.9.0.0/24 }
  back: { cidr: 10.8.0.0/24 }
groups:
  servers: { members: [web, dc, box] }
machines:
  web:
    networks: { back: 10, lab: 10 }
    os: debian@12
  dc:
    networks: { lab: 20 }
    os: windows-server@2022
  box:
    networks: { lab: 30 }
    app: { image: alpine }
provision:
  - ansible: site.yml
    groups: { dcs: [dc], boxes: [box] }
    host_vars:
      web: { code: '10', quote: it's, n: 3, on: true, none: null, l: [x, 1], m: { k: v } }
overrides:
  vagrant: { machines: { dc: { box: some/box, winrm: ssl } } }
  external:
    machines:
      web: { ssh: { address: web.example.org, user: ops, port: 2222, key: ~/my keys/id } }
      dc: { ssh: { address: 192.168.1.20 } }
";

/// The lines of section `[name]` of inventory `inv`, up to the next section (its blank line
/// kept); empty when it has no such section.
#[must_use]
pub fn section<'a>(inv: &'a str, name: &str) -> &'a str {
    let Some((_, rest)) = inv.split_once(&format!("[{name}]\n")) else { return "" };
    rest.split_inclusive("\n[").next().map_or(rest, |s| s.strip_suffix('[').unwrap_or(s))
}

/// The repository's example `name`, planned for `target`.
///
/// # Errors
///
/// Why it doesn't load, its problems, or why it can't be planned.
pub fn example(name: &str, target: isoloom_spec::model::Target) -> Result<isoloom_plan::Environment, String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    let lab = isoloom_plan::Lab::new(isoloom_spec::load(&dir).map_err(|e| e.to_string())?);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("{name} isn't valid: {problems:?}"));
    }
    isoloom_plan::Environment::build(&lab.for_target(target), target, None).map_err(|e| e.to_string())
}

/// The plan of `yaml` for `target`.
///
/// # Errors
///
/// Why the spec doesn't parse, or can't be planned.
pub fn planned(yaml: &str, target: isoloom_spec::model::Target) -> Result<isoloom_plan::Environment, String> {
    let lab = isoloom_plan::Lab::new(isoloom_spec::parse(yaml).map_err(|e| e.to_string())?);
    isoloom_plan::Environment::build(&lab.for_target(target), target, None).map_err(|e| e.to_string())
}

/// A sensitive input slot `name` delivered to every step as the environment variable `var`.
///
/// # Errors
///
/// A name or variable the manifest refuses.
pub fn slot(name: &str, var: &str) -> Result<isoloom_manifest::SecretSlot, isoloom_manifest::Error> {
    use isoloom_manifest::{Delivery, EnvName, Identifier, NonEmpty, SecretSlot, SlotKind, StepSelection};
    Ok(SecretSlot {
        name: Identifier::parse(name)?,
        kind: SlotKind::Input,
        required: false,
        sensitive: true,
        machines: Vec::new(),
        providers: std::collections::BTreeSet::new(),
        source: None,
        default: None,
        deliver: NonEmpty::one(Delivery::Env {
            var: EnvName::parse(var)?,
            steps: StepSelection::Every,
        }),
    })
}
