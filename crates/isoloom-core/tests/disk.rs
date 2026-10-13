//! `resources.disk_gb` on the Vagrant providers: the VM's disk sized (a minimum), the root
//! filesystem grown into it before the machine's steps.

use isoloom_core::{Target, generate, parse};

fn vagrantfile(disk: &str) -> String {
    let spec = parse(&format!(
        "version: 1
name: disks
networks: {{ lab: {{ cidr: 10.77.0.0/24 }} }}
machines:
  box:
    networks: {{ lab: 10 }}
    resources: {{ cpus: 1, memory_mb: 1024{disk} }}
    vm: {{ os: debian-12, provision: [setup.sh] }}
"
    ))
    .unwrap();
    generate(&spec, Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents
}

#[test]
fn each_provider_sizes_the_disk_and_the_root_grows_before_the_steps() {
    let vf = vagrantfile(", disk_gb: 80");
    assert!(vf.contains("m.vm.disk :disk, size: \"80GB\", primary: true"), "VirtualBox, VMware, Hyper-V");
    assert!(vf.contains("v.machine_virtual_size = 80"), "libvirt");
    assert!(vf.contains("v.guest_boot_disk_size = 80"), "ESXi");
    assert!(!vf.contains("disk_resize"), "QEMU would truncate a disk below the box's");
    let grow = vf.find("name: \"disk\"").expect("the grow step");
    let steps = vf.find("name: \"setup.sh\"").unwrap();
    assert!(grow < steps, "the root is grown before the machine's steps");
    assert!(vf.contains("lvextend -l +100%FREE") && vf.contains("growpart"));
}

#[test]
fn without_a_disk_the_box_keeps_its_own() {
    let vf = vagrantfile("");
    for absent in ["m.vm.disk", "machine_virtual_size", "guest_boot_disk_size", "name: \"disk\""] {
        assert!(!vf.contains(absent), "{absent}");
    }
}
