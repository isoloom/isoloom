//! `resources.disk_gb` on the Vagrant providers: the VM's disk sized (a minimum), the root
//! filesystem grown into it before the machine's steps.

use isoloom_spec::Target;

use crate::testing::{Outcome, generated, lab};

/// The Vagrantfile of a one-machine environment whose `resources` end with `disk`.
fn vagrantfile(disk: &str) -> Outcome<String> {
    let lab = lab(&format!(
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
    ))?;
    generated(&lab, Target::Vagrant, "Vagrantfile")
}

#[test]
fn each_provider_sizes_the_disk_and_the_root_grows_before_the_steps() -> Outcome {
    let vf = vagrantfile(", disk_gb: 80")?;
    assert!(vf.contains("m.vm.disk :disk, size: \"80GB\", primary: true"), "VirtualBox, VMware, Hyper-V");
    assert!(vf.contains("v.machine_virtual_size = 80"), "libvirt");
    assert!(vf.contains("v.guest_boot_disk_size = 80"), "ESXi");
    // QEMU: only above the box's own size (a smaller overlay would truncate it).
    assert!(vf.contains("v.disk_resize = IsoloomQemu.disk(\"cloud-image/debian-12\", 80)"), "QEMU");
    let grow = vf.find("name: \"disk\"").expect("the grow step");
    let steps = vf.find("name: \"setup.sh\"").unwrap();
    assert!(grow < steps, "the root is grown before the machine's steps");
    assert!(vf.contains("lvextend -l +100%FREE") && vf.contains("growpart"));
    Ok(())
}

#[test]
fn without_a_disk_the_box_keeps_its_own() -> Outcome {
    let vf = vagrantfile("")?;
    for absent in ["m.vm.disk", "machine_virtual_size", "guest_boot_disk_size", "name: \"disk\""] {
        assert!(!vf.contains(absent), "{absent}");
    }
    Ok(())
}

/// docker-vm's VM on QEMU takes the containers' host disk, as cloud-docker's and Proxmox's do: at
/// least 40 GB, more when the environment's containers ask for more.
#[test]
fn docker_vms_qemu_disk_is_the_containers_host_disk() -> Outcome {
    let with_disk = |gb: u32| {
        lab(&format!(
            "version: 1
name: disks
networks: {{ lab: {{ cidr: 10.77.0.0/24 }} }}
machines:
  web:
    networks: {{ lab: 10 }}
    app: {{ image: nginx }}
    resources: {{ disk_gb: {gb} }}
"
        ))
    };
    let small = generated(&with_disk(5)?, Target::DockerVm, "Vagrantfile")?;
    assert!(small.contains("IsoloomQemu.disk(\"cloud-image/debian-12\", 40)"), "{small}");
    let large = generated(&with_disk(70)?, Target::DockerVm, "Vagrantfile")?;
    assert!(large.contains("IsoloomQemu.disk(\"cloud-image/debian-12\", 70)"), "{large}");
    Ok(())
}
