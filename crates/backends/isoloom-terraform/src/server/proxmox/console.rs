//! Each VM's graphical console on Proxmox: Proxmox VE's own, through its API (`vncproxy`, then
//! `vncwebsocket`), never a VNC port of the server's. The module gives each VM's id
//! (`vm_id_<machine>`), which the console is asked for by, with the connection Terraform uses.
//! An Arm VM has none: its console is the serial port (no display, [`super::arch`]).

use isoloom_plan::Environment;

use crate::hcl_builder::{Module, reference};
use crate::module::{ident, output};

/// The output holding `machine`'s VM id.
fn vm_id_output(machine: &str) -> String {
    format!("vm_id_{}", ident(machine).to_ascii_lowercase())
}

/// The VMs that have a display, by name: every one but an Arm VM.
fn shown(env: &Environment) -> impl Iterator<Item = &str> {
    env.vms().filter(|vm| !super::arch::is_arm(vm.plan)).map(|vm| vm.name)
}

/// The module's outputs of each displayed VM's id, which its console is asked for by.
pub(super) fn outputs(env: &Environment, module: &mut Module) {
    for name in shown(env) {
        module.commented(
            &format!("{name}'s VM id: its console is asked for by it (Proxmox's vncproxy)."),
            output(&vm_id_output(name), reference(&format!("proxmox_virtual_environment_vm.{}.vm_id", ident(name)))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vm_gives_its_id_for_its_console_and_an_arm_vm_none() {
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
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        let body = isoloom_test_support::hcl_body(&m.render().unwrap());
        let outputs: Vec<(String, String)> = isoloom_test_support::hcl_blocks(&body, "output", &[])
            .iter()
            .map(|o| {
                let name = o.labels().first().map(|l| l.as_str().to_owned()).unwrap_or_default();
                (name, isoloom_test_support::hcl_text(o.body(), "value"))
            })
            .collect();
        assert_eq!(outputs, [("vm_id_web".to_owned(), "proxmox_virtual_environment_vm.web.vm_id".to_owned())]);
    }
}
