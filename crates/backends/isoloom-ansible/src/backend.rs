//! The `external` target as a backend: what hosts that already exist can run, and the files
//! that reach them.

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_render::inventory::{self, Access};
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
        // Delivered by `isoloom run` over the host's SSH, owners included.
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
    let inventory = format!("{}{}", header("#"), inventory::render(env, &Access::external()));
    let mut files = vec![output("inventory.ini", inventory), output("machines.json", endpoints::json(env))];
    files.extend(checks::runners(env).into_iter().map(|(path, script)| output(&path, script)));
    // The Ansible CM project (`site.yml`, the machine roles and `requirements.yml`) that
    // provisions the existing hosts: the recipe's steps as native tasks, run over this inventory.
    files.extend(crate::configure::project(env, &format!("{OUTPUT_DIR}/{DIR}")));
    Ok(files)
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
