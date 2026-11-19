//! `resources.disk_gb` on the Vagrant providers: the VM's disk sized (a minimum), the root
//! filesystem grown into it before the machine's steps.

use isoloom_test_support::RubyBlock;

use crate::testing::{Outcome, define, lab, provisioners, vagrantfile};

/// The Vagrantfile of a one-machine environment whose `resources` end with `disk`, read into
/// its blocks.
fn disks(disk: &str) -> Outcome<RubyBlock> {
    vagrantfile(&lab(&format!(
        "version: 1
name: disks
networks: {{ lab: {{ cidr: 10.77.0.0/24 }} }}
machines:
  box:
    os: debian@12
    networks: {{ lab: 10 }}
    resources: {{ cpus: 1, memory_mb: 1024{disk} }}
    app: {{ steps: [{{ run: setup.sh }}] }}
"
    ))?)
}

/// The statements of provider `provider`'s block in VM block `vm`.
fn provider<'v>(vm: &'v RubyBlock, provider: &str) -> Vec<&'v str> {
    vm.block(&format!("m.vm.provider \"{provider}\"")).every_statement()
}

#[test]
fn each_provider_sizes_the_disk_and_the_root_grows_before_the_steps() -> Outcome {
    let vf = disks(", disk_gb: 80")?;
    let vm = define(&vf, "box");
    // VirtualBox and VMware grow the box's disk with Vagrant's own setting, in their blocks;
    // Hyper-V, where that setting would shrink a bigger VHDX, with Isoloom's step before the VM
    // starts, which grows it only when smaller.
    assert!(vm.statements("m.vm.disk").is_empty(), "no disk setting every provider reads");
    for p in ["virtualbox", "vmware_desktop"] {
        assert!(provider(vm, p).contains(&"o.vm.disk :disk, size: \"80GB\", primary: true"), "{p}");
    }
    assert!(!provider(vm, "hyperv").iter().any(|s| s.contains("vm.disk")), "Hyper-V reads no disk setting");
    assert!(
        vm.block("m.trigger.before :\"VagrantPlugins::HyperV::Action::StartInstance\"")
            .every_statement()
            .iter()
            .any(|s| s.contains("IsoloomHyperV.grow(machine, 80)")),
        "Hyper-V grows it before the VM starts"
    );
    assert!(provider(vm, "libvirt").contains(&"v.machine_virtual_size = 80"), "libvirt");
    assert!(provider(vm, "vmware_esxi").contains(&"v.guest_boot_disk_size = 80"), "ESXi");
    // QEMU: only above the box's own size (a smaller overlay would truncate it).
    assert!(
        provider(vm, "qemu").contains(&"v.disk_resize = IsoloomQemu.disk(\"cloud-image/debian-12\", 80)"),
        "QEMU"
    );
    let steps = provisioners(vm);
    let at = |name: &str| steps.iter().position(|s| s == name).ok_or_else(|| format!("no `{name}` step in {steps:?}"));
    assert!(at("disk")? < at("setup.sh")?, "the root is grown before the machine's steps: {steps:?}");
    let grow = vm.statements("m.vm.provision \"shell\", name: \"disk\"");
    assert!(
        matches!(grow.as_slice(), [one] if one.contains("lvextend -l +100%FREE") && one.contains("growpart")),
        "{grow:?}"
    );
    Ok(())
}

#[test]
fn without_a_disk_the_box_keeps_its_own() -> Outcome {
    let vf = disks("")?;
    let vm = define(&vf, "box");
    assert!(!provisioners(vm).contains(&"disk".to_owned()), "no grow step");
    let statements = vm.every_statement();
    for absent in ["m.vm.disk", "o.vm.disk", "v.machine_virtual_size", "v.guest_boot_disk_size", "v.disk_resize"] {
        assert!(!statements.iter().any(|s| s.starts_with(absent)), "{absent}");
    }
    assert!(!statements.iter().any(|s| s.contains("IsoloomHyperV.grow")), "no Hyper-V grow");
    Ok(())
}
