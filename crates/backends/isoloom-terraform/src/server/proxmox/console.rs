//! Each VM's graphical console on Proxmox: Proxmox VE's own, through its API (`vncproxy`, then
//! `vncwebsocket`), never a VNC port of the server's. The module gives each VM's id
//! (`vm_id_<machine>`), and the run manifest says where the API is and what signs in to it: the
//! connection's own params and slots, the ones Terraform uses. An Arm VM has none: its console
//! is the serial port (no display, [`super::arch`]).

use std::collections::BTreeSet;

use isoloom_manifest::{
    Console, ConsoleProtocol, ConsoleSource, Error as ManifestError, Identifier, Machine, Output, OutputRole, Placeholder, SnakeName, Template,
};
use isoloom_plan::Environment;

use crate::hcl_builder::{Module, reference};
use crate::module::{ident, output};

/// The output holding `machine`'s VM id.
fn vm_id_output(machine: &str) -> String {
    format!("vm_id_{}", ident(machine).to_ascii_lowercase())
}

/// Why an Arm VM has no graphical console.
const SERIAL: &str = "an Arm VM on Proxmox has no display: its console is the serial port";

/// The VMs that have a display, by name: every one but an Arm VM.
fn shown(env: &Environment) -> impl Iterator<Item = &str> {
    env.vms().filter(|vm| !super::arch::is_arm(vm.plan)).map(|vm| vm.name)
}

/// The module's outputs of each displayed VM's id, which its console is asked for by.
pub(super) fn outputs(env: &Environment, m: &mut Module) {
    for name in shown(env) {
        m.commented(
            &format!("{name}'s VM id: its console is asked for by it (Proxmox's vncproxy)."),
            output(&vm_id_output(name), reference(&format!("proxmox_virtual_environment_vm.{}.vm_id", ident(name)))),
        );
    }
}

/// Each VM's console among `machines`, and the outputs it reads (`outputs`).
pub(super) fn add(env: &Environment, machines: &mut [Machine], outputs: &mut Vec<Output>) -> Result<(), ManifestError> {
    let param = |name: &str| -> Result<Template, ManifestError> { Ok(Template::from(Placeholder::Param(isoloom_manifest::EnvName::parse(name)?))) };
    let with_display: BTreeSet<&str> = shown(env).collect();
    for m in machines.iter_mut() {
        let Some(vm) = env.vm(m.name.as_str()) else { continue };
        if !with_display.contains(vm.name) {
            m.consoles = vec![Console::none(BTreeSet::new(), SERIAL)];
            continue;
        }
        let id = vm_id_output(vm.name);
        let source = ConsoleSource::ProxmoxApi {
            endpoint: param("proxmox_endpoint")?,
            node: param("node")?,
            vm_id: Template::from(Placeholder::Output(SnakeName::parse(&id)?)),
            insecure: param("proxmox_insecure")?,
            username: param("proxmox_username")?,
            token: Identifier::parse("proxmox_api_token")?,
            password: Identifier::parse("proxmox_password")?,
        };
        m.consoles = vec![Console::at(BTreeSet::new(), ConsoleProtocol::Vnc, source)];
        outputs.push(crate::module::manifest::terraform_output(
            &id,
            OutputRole::Info,
            &format!("{}'s VM id on the node: its console (Proxmox's vncproxy) is asked for by it.", vm.name),
        )?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use isoloom_manifest::ConsoleProtocol;

    use super::*;

    #[test]
    fn a_vm_has_proxmoxs_console_and_an_arm_vm_none() {
        let env = crate::testing::environment(
            "version: 1
name: shop
networks: { lab: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lab: 10 }, os: debian@12 }
  pi: { networks: { lab: 11 }, os: debian@12, hardware: { arch: arm64 } }
",
            isoloom_spec::model::Target::Proxmox,
        );
        let mut machines = vec![
            isoloom_render::run_manifest::machine("web", isoloom_manifest::Role::Member).unwrap(),
            isoloom_render::run_manifest::machine("pi", isoloom_manifest::Role::Member).unwrap(),
            isoloom_render::run_manifest::machine("isoloom-router", isoloom_manifest::Role::Infra).unwrap(),
        ];
        let mut outputs = Vec::new();
        add(&env, &mut machines, &mut outputs).unwrap();
        let web = &machines[0].consoles[0];
        assert_eq!(web.protocol, ConsoleProtocol::Vnc);
        let Some(ConsoleSource::ProxmoxApi { vm_id, endpoint, token, .. }) = &web.source else {
            panic!("{web:?}")
        };
        assert_eq!(
            (vm_id.to_string(), endpoint.to_string()),
            ("${outputs.vm_id_web}".to_owned(), "${params.proxmox_endpoint}".to_owned())
        );
        assert_eq!(token.as_str(), "proxmox_api_token");
        assert_eq!(machines[1].consoles[0].protocol, ConsoleProtocol::None);
        assert!(machines[2].consoles.is_empty(), "the router is Isoloom's own");
        assert_eq!(outputs.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["vm_id_web"]);
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        let text = m.render().unwrap();
        assert!(
            text.contains("output \"vm_id_web\"") && text.contains("proxmox_virtual_environment_vm.web.vm_id"),
            "{text}"
        );
        assert!(!text.contains("vm_id_pi"), "{text}");
    }
}
