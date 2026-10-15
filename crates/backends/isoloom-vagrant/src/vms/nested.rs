//! Nested virtualization (`nested_virtualization`): each provider's own switch for a VM that
//! runs VMs of its own, written in its provider block. QEMU and UTM on a Mac have none, so a
//! `vagrant up` on them stops with the reason before creating the VM, rather than boot a VM
//! that can't do what the spec asks.

use super::provider::Provider;
use crate::ruby::{Call, Ruby, Value};

/// Why the Mac's providers stop: they run on Apple's hypervisor, which nests no VMs.
const NO_NESTING: &str = "nested virtualization (`nested_virtualization`): QEMU and UTM run on the Mac's Hypervisor framework, which has none; use VirtualBox, VMware, Parallels, libvirt or Hyper-V";

/// The provider block's switch for `provider`, if it has one.
pub(crate) fn switch(r: &mut Ruby, provider: Provider<'_>) {
    match provider {
        Provider::VirtualBox => {
            r.call(Call::new("v.customize").arg(modify("modifyvm", "--nested-hw-virt")));
        }
        Provider::Parallels => {
            r.call(Call::new("v.customize").arg(modify("set", "--nested-virt")));
        }
        Provider::VmwareDesktop => {
            r.assign("v.vmx[\"vhv.enable\"]", Value::str("TRUE"));
        }
        Provider::VmwareEsxi => {
            r.assign(
                "v.guest_custom_vmx_settings",
                Value::array([Value::array([Value::str("vhv.enable"), Value::str("TRUE")])]),
            );
        }
        Provider::Libvirt => {
            r.assign("v.nested", true);
        }
        Provider::HyperV => {
            r.assign("v.enable_virtualization_extensions", true);
        }
        // The Mac's providers stop the VM instead (`refuse_elsewhere`).
        Provider::Utm | Provider::Qemu | Provider::Other(_) => {}
    }
}

/// `[<command>, :id, <flag>, "on"]`: VirtualBox's and Parallels' command lines.
fn modify(command: &str, flag: &str) -> Value {
    Value::array([Value::str(command), Value::sym("id"), Value::str(flag), Value::str("on")])
}

/// A trigger stopping `vagrant up` on the providers that can't nest, before the VM is created.
pub(crate) fn refuse_elsewhere(r: &mut Ruby) {
    r.block(Call::new("m.trigger.before").arg(Value::sym("up")), "t", |r| {
        r.block("t.ruby", "_, machine", |r| {
            r.line(format_args!(
                "abort(\"#{{machine.name}}: \" + {}) if %i[qemu utm].include?(machine.provider_name)",
                Value::str(NO_NESTING)
            ));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruby(f: impl FnOnce(&mut Ruby)) -> String {
        let mut r = Ruby::new();
        f(&mut r);
        r.finish()
    }

    #[test]
    fn each_provider_has_its_own_switch() {
        let out = |p: &str| ruby(|r| switch(r, Provider::from(p)));
        assert_eq!(out("virtualbox"), "v.customize [\"modifyvm\", :id, \"--nested-hw-virt\", \"on\"]\n");
        assert_eq!(out("parallels"), "v.customize [\"set\", :id, \"--nested-virt\", \"on\"]\n");
        assert_eq!(out("vmware_desktop"), "v.vmx[\"vhv.enable\"] = \"TRUE\"\n");
        assert_eq!(out("vmware_esxi"), "v.guest_custom_vmx_settings = [[\"vhv.enable\", \"TRUE\"]]\n");
        assert_eq!(out("libvirt"), "v.nested = true\n");
        assert_eq!(out("hyperv"), "v.enable_virtualization_extensions = true\n");
        assert_eq!(out("qemu"), "");
    }

    #[test]
    fn the_mac_providers_stop_before_creating_the_vm() {
        let out = ruby(refuse_elsewhere);
        assert!(
            out.starts_with("m.trigger.before :up do |t|\n  t.ruby do |_, machine|\n    abort(\"#{machine.name}: \" + \"nested virtualization"),
            "{out}"
        );
        assert!(out.contains("if %i[qemu utm].include?(machine.provider_name)\n"), "{out}");
    }
}
