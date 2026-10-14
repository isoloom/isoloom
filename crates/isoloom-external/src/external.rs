//! The `external` target: machines that already exist (a hand-built Proxmox or ESXi set of
//! VMs, a home lab, hardware). Isoloom creates nothing; it provisions, checks and reaches them
//! over SSH at the addresses the spec gives (`machines.*.external`), in `.isoloom/external/`:
//!
//! - `inventory.ini`: every machine with its `external` address, user, port and key, in the
//!   `linux` / `windows` groups and the inventory groups, for `provision:` playbooks run from
//!   here ([`isoloom_render::inventory`] with [`Access::external`]).
//! - `checks/<machine>.sh`: the check runner of each machine that has an address (declared and
//!   derived probes; scripts need the project on the machine and are left out, with a note).
//! - `machines.json`: name -> address, user, port, key, for `isoloom run/test/connect/exec`.
//!
//! Networks and `reach` are expected behavior here (Isoloom adds no router), which the derived
//! checks verify. Everything is rendered from the plan ([`Environment`]).

use isoloom_plan::checks::{self, Position, Probe, Resolved};
use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{Backend, Files, RenderError, check};
use isoloom_render::inventory::{self, Access};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::model::Target;
use serde_json::{Map, Value, json};

const DIR: &str = "external";

/// The `external` backend.
pub struct ExternalBackend;

impl Backend for ExternalBackend {
    fn target(&self) -> Target {
        Target::External
    }

    /// Provisioning runs from here with the inventory; checks are probes from each machine (a
    /// playbook or script check is left out of the runners, not refused). A gateway machine is
    /// one of the existing hosts, routing as it already does. Appliances need Docker's networks,
    /// which existing hosts don't have.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of([
            Feature::EnvironmentProvisioning,
            Feature::PlaybookChecks,
            Feature::ExecWithoutContainer,
            Feature::Gateways,
            Feature::MachineAliases,
            Feature::Ulimits,
            Feature::TrafficControl,
        ])
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render(env)
    }
}

/// The files of the `external` target for `env` (planned for [`Target::External`]).
pub fn render(env: &Environment) -> Result<Files, RenderError> {
    check(Target::External, env, &ExternalBackend.capabilities())?;
    let mut files = vec![
        file("inventory.ini", format!("{}{}", header("#"), inventory::render(env, &Access::external()))),
        file("machines.json", machines(env)),
    ];
    files.extend(check_runners(env));
    Ok(files)
}

fn file(path: &str, contents: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/{path}"),
        contents,
    }
}

/// `machines.json`: each existing host's SSH endpoint, defaults filled (`root`, port 22).
fn machines(env: &Environment) -> String {
    let machines: Map<String, Value> = env
        .machines
        .iter()
        .filter_map(|(name, m)| {
            let e = m.external.as_ref()?;
            let endpoint = json!({
                "address": e.address,
                "user": e.user.as_deref().unwrap_or("root"),
                "port": e.port.unwrap_or(22),
                "key": e.key,
            });
            Some((name.clone(), endpoint))
        })
        .collect();
    format!("{:#}\n", Value::Object(machines))
}

/// One runner per existing host with probes to run from it; scripts and playbooks need the
/// project on the machine and are left out.
fn check_runners(env: &Environment) -> Vec<GeneratedFile> {
    let host = |h: &checks::Host, _: &Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => env
                .machines
                .get(name)
                .and_then(|m| m.addresses.get(network))
                .map_or_else(|| name.clone(), ToString::to_string),
        }
    };
    let no_script = |_: &str| "echo 'scripts need the project on the machine; the external target runs probes only' >&2; exit 1".to_string();
    let render = checks::Render {
        host: &host,
        script: &no_script,
        playbook: None,
    };
    checks::by_position(env.spec(), &env.checks)
        .into_iter()
        .filter_map(|(pos, group)| {
            let Position::Machine(m) = &pos else { return None };
            env.machines.get(m)?.external.as_ref()?;
            let probes: Vec<&Resolved> = group
                .into_iter()
                .filter(|c| !matches!(c.probe, Probe::Script { .. } | Probe::Playbook { .. }))
                .collect();
            (!probes.is_empty()).then(|| file(&format!("checks/{m}.sh"), checks::script(&pos, &probes, &render)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n";

    fn env(machines: &str) -> Environment {
        let spec = isoloom_spec::parse(&format!("{BASE}{machines}")).expect("parses");
        Environment::build(&spec, Target::External, None)
    }

    #[test]
    fn an_appliance_is_refused_from_the_capabilities_without_rendering() {
        let env = env("  r: { networks: { lab: 1 }, docker: { image: x, appliance: cisco-iol }, external: { address: 10.0.0.1 } }\n");
        let refusal = ExternalBackend.refusal(&env).expect("refused");
        assert!(refusal.contains("network appliance"), "{refusal}");
        assert!(matches!(render(&env), Err(RenderError::Refused { .. })));
    }

    #[test]
    fn machines_json_fills_the_ssh_defaults() {
        let env = env("  a: { networks: { lab: 5 }, vm: { os: debian-12 }, external: { address: h.example, port: 2200 } }\n");
        let files = render(&env).expect("renders");
        let json = &files.iter().find(|f| f.path.ends_with("machines.json")).expect("machines.json").contents;
        assert_eq!(
            json,
            "{\n  \"a\": {\n    \"address\": \"h.example\",\n    \"user\": \"root\",\n    \"port\": 2200,\n    \"key\": null\n  }\n}\n"
        );
        let inv = &files.iter().find(|f| f.path.ends_with("inventory.ini")).expect("inventory").contents;
        assert!(inv.contains("\na ansible_host=h.example ansible_port=2200\n"), "{inv}");
    }
}
