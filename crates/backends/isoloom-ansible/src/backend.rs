//! The `external` target as a backend: what hosts that already exist can run, and the files
//! that reach them.

use isoloom_plan::{Capabilities, Environment, Feature, MachinePlan};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_render::inventory::{self, Access, Vars};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::model::Target;

use crate::error::{Error, Result};
use crate::{checks, endpoints};

/// The target's folder, under `.isoloom/`.
const DIR: &str = "external";

/// The `external` target's backend: the inventory, the hosts' endpoints and the check runners
/// of an environment on hosts that already exist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ExternalBackend;

impl Backend for ExternalBackend {
    fn target(&self) -> Target {
        Target::External
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        Ok(render(env)?)
    }
}

/// What hosts that already exist can run. Provisioning runs from the project with the
/// inventory; checks are probes from each machine (a playbook or script check is left out of
/// the runners, not refused). A gateway machine is one of the existing hosts, routing as it
/// already does. Appliances need Docker's networks, which existing hosts don't have.
fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        // The hosts exist with their addresses: an IPv6 network adds nothing to build. An
        // IPv6-only one would need their IPv6 addresses, which the spec doesn't give.
        Feature::Ipv6,
        Feature::Ipv6Auto,
        // The hosts are already on their networks, outside ones included.
        Feature::ExternalNetworks,
        Feature::BridgedNetworks,
        // Delivered by the runner over the host's SSH, owners included.
        Feature::Secrets,
        Feature::SecretOwners,
    ])
}

/// The files of `env` (planned for [`Target::External`]): the inventory, the hosts'
/// endpoints, then a check runner per host with probes to run. Refused, without rendering,
/// when `env` needs what existing hosts can't give.
fn render(env: &Environment) -> Result<Files> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Err(Error::Refused { refusal });
    }
    let inventory = format!("{}{}", header("#"), inventory::render(env, &access()));
    let mut files = vec![output("inventory.ini", inventory), output("machines.json", endpoints::json(env))];
    files.extend(checks::runners(env).into_iter().map(|(path, script)| output(&path, script)));
    // The Ansible CM project (`site.yml`, the machine roles and `requirements.yml`) that
    // provisions the existing hosts: the recipe's steps as native tasks, run over this inventory.
    files.extend(crate::configure::project(env, &format!("{OUTPUT_DIR}/{DIR}"))?);
    Ok(files)
}

/// How Ansible reaches the existing hosts: each one's SSH user, port and key as its `external:`
/// gives them; Windows hosts over SSH with PowerShell (OpenSSH on Windows), not WinRM.
pub(crate) fn access() -> Access<'static> {
    Access::new(Vars::new(), inventory::vars(&inventory::WINDOWS_OVER_SSH), |h| external_vars(h.machine))
}

/// An existing host's `ansible_user`, `ansible_port` and key: the ones its `external:` sets.
fn external_vars(m: &MachinePlan) -> Vars {
    let Some(e) = &m.external else { return Vars::new() };
    let mut out = Vars::new();
    if let Some(u) = &e.user {
        out.push(("ansible_user".into(), u.clone()));
    }
    if let Some(p) = e.port {
        out.push(("ansible_port".into(), p.to_string()));
    }
    if let Some(k) = &e.key {
        out.push(("ansible_ssh_private_key_file".into(), k.clone()));
    }
    out
}

/// The file at `path` in the target's folder.
fn output(path: &str, contents: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/{path}"),
        contents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn an_appliance_is_refused_from_the_capabilities_without_rendering() {
        let env = environment(
            "  r: { networks: { lab: 1 }, app: { image: x, appliance: cisco-iol } }\noverrides: { external: { machines: { r: { ssh: { address: 10.0.0.1 } } } } }\n",
        );
        let refusal = ExternalBackend.refusal(&env).expect("refused");
        assert!(refusal.contains("network appliance"), "{refusal}");
        assert!(matches!(ExternalBackend.render(&env), Err(RenderError::Refused { .. })));
    }

    #[test]
    fn rendering_twice_gives_the_same_files() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12 }\n  b: { networks: { lab: 6 }, os: debian@12 }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } }, b: { ssh: { address: 10.0.0.6 } } } } }\n",
        );
        assert_eq!(render(&env).expect("renders"), render(&env).expect("renders again"));
    }

    #[test]
    fn the_inventory_reaches_each_host_at_its_own_address_and_port() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12 }\noverrides: { external: { machines: { a: { ssh: { address: h.example, port: 2200 } } } } }\n",
        );
        let files = render(&env).expect("renders");
        let inventory = files.iter().find(|f| f.path == ".isoloom/external/inventory.ini").expect("the inventory");
        assert!(
            inventory.contents.contains("\na ansible_host=h.example ansible_port=2200\n"),
            "{}",
            inventory.contents
        );
    }
}

/// The inventory as the existing hosts are reached.
#[cfg(test)]
mod inventory_tests {
    use isoloom_plan::Environment;
    use isoloom_render::inventory::render;
    use isoloom_spec::model::Target;

    use super::access;

    const SPEC: &str = r"
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

    /// The lines of section `[name]`, up to the next section (its blank line kept).
    fn section<'a>(inv: &'a str, name: &str) -> &'a str {
        let (_, rest) = inv.split_once(&format!("[{name}]\n")).unwrap_or_else(|| panic!("[{name}] in\n{inv}"));
        rest.split_inclusive("\n[").next().map_or(rest, |s| s.strip_suffix('[').unwrap_or(s))
    }

    fn planned(yaml: &str) -> Environment {
        let lab = isoloom_plan::Lab::new(isoloom_spec::parse(yaml).expect("the spec parses"));
        Environment::build(&lab.for_target(Target::External), Target::External, None)
    }

    /// Example `name` of the project, which must be valid, planned for `target`.
    fn example(name: &str, target: Target) -> Environment {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
        let lab = isoloom_plan::Lab::new(isoloom_spec::load(&dir).expect("the example loads"));
        assert_eq!(lab.problems(), [], "{name} is valid");
        Environment::build(&lab.for_target(target), target, None)
    }

    #[test]
    fn external_hosts_carry_their_user_port_and_quoted_key() {
        let inv = render(&planned(SPEC), &access());
        assert!(
            inv.starts_with("[linux]\nweb ansible_host=web.example.org ansible_user=ops ansible_port=2222 ansible_ssh_private_key_file='~/my keys/id' code="),
            "{inv}"
        );
        assert_eq!(section(&inv, "windows"), "dc ansible_host=192.168.1.20\n\n");
        assert_eq!(section(&inv, "windows:vars"), "ansible_connection=ssh\nansible_shell_type=powershell\n\n");
        assert_eq!(section(&inv, "linux:vars"), "ansible_become=true\n\n");
    }

    #[test]
    fn external_existing_hosts() {
        insta::assert_snapshot!(render(&example("existing-hosts", Target::External), &access()));
    }

    #[test]
    fn rendering_twice_gives_the_same_inventory() {
        let once = render(&example("existing-hosts", Target::External), &access());
        assert_eq!(once, render(&example("existing-hosts", Target::External), &access()));
    }
}
