//! Each cloud VM's graphical console. A cloud gives none of its own that Isoloom reaches (a
//! serial console at most), so a Windows VM's is its own Remote Desktop, and only when the spec
//! opens its port (3389/tcp, which the set-up turns Remote Desktop on for), reached through a
//! Linux machine of the lab on a network the Windows one is on, never at a public address. It
//! signs in as the administrator Isoloom makes (`isoloom`), with the generated password the
//! module gives as a sensitive output ([`PASSWORD_OUTPUT`]). A Linux VM has none: its way in is
//! SSH.

use isoloom_plan::{Environment, Vm};

use crate::hcl_builder::{Block, Module, boolean, reference};
use crate::machine::windows::PASSWORD;

/// Remote Desktop's port.
pub(crate) const RDP_PORT: u16 = 3389;

/// The sensitive output holding the Windows machines' password, for Remote Desktop.
pub(crate) const PASSWORD_OUTPUT: &str = "windows_password";

/// Whether `vm` is a Windows machine whose spec opens Remote Desktop's port.
pub(crate) fn opens_rdp(env: &Environment, vm: Vm<'_>) -> bool {
    vm.plan.is_windows()
        && env
            .spec()
            .machines
            .get(vm.name)
            .is_some_and(|m| m.ports.iter().any(|p| p.port == RDP_PORT && p.protocol.is_tcp()))
}

/// The module's output of the Windows machines' password, when a Windows machine's Remote
/// Desktop is reached: sensitive, so `terraform output` hides it and the runner masks it.
pub(crate) fn outputs(env: &Environment, module: &mut Module) {
    if env.vms().any(|vm| opens_rdp(env, vm)) {
        module.commented(
            "The Windows machines' administrator password (user isoloom), for their Remote Desktop.",
            Block::builder("output")
                .add_label(PASSWORD_OUTPUT)
                .add_attribute(("value", reference(PASSWORD)))
                .add_attribute(("sensitive", boolean(true)))
                .build(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "version: 1
name: shop
networks: { lab: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lab: 10 }, os: debian@12 }
  dc: { networks: { lab: 20 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
";

    #[test]
    fn a_windows_vm_opening_rdp_gets_a_sensitive_password_output() {
        let env = crate::testing::environment(SPEC, isoloom_spec::model::Target::CloudVm);
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        let text = m.render().unwrap();
        assert!(text.contains("output \"windows_password\"") && text.contains("sensitive = true"), "{text}");
    }

    #[test]
    fn without_a_windows_vm_opening_rdp_there_is_no_password_output() {
        let env = crate::testing::environment(
            &SPEC.replace("ports: { rdp: { port: 3389 } }", "ports: { web: { port: 80 } }"),
            isoloom_spec::model::Target::CloudVm,
        );
        let mut m = Module::default();
        super::outputs(&env, &mut m);
        assert_eq!(m.render().unwrap(), "");
    }
}
