//! The Vagrantfile's settings (machine settings, network types, provisioners, providers)
//! and whether an Isoloom spec can produce them. Vagrant has no schema: the list follows
//! the Vagrant documentation.

use super::{Format, Support, Support::*};

const TOOLING: Support = Tooling {
    note: "Vagrant tooling, not the environment's behavior",
};
const HOST_SIDE: Support = ByDesign {
    why: "would expose the environment on the user's machine or LAN",
};
const VIA_SHELL: Support = Equivalent {
    via: "a `.sh` provisioning step can run any tool",
};

pub fn format() -> Format {
    Format {
        name: "Vagrant",
        file: ".isoloom/vagrant/Vagrantfile",
        source: "the Vagrant documentation (machine settings, networking, provisioners, providers)",
        rows: rows(),
    }
}

fn rows() -> Vec<(&'static str, Support)> {
    vec![
        // Machine settings (config.vm.*).
        ("config.vm.define", Emitted { from: "machines" }),
        (
            "config.vm.box",
            Partial {
                from: "machines.*.vm.os",
                gap: "Linux boxes only; Windows comes later",
            },
        ),
        (
            "config.vm.box_version",
            Planned {
                note: "pinning images to a version",
            },
        ),
        (
            "config.vm.box_architecture",
            Planned {
                note: "a CPU architecture field (amd64, arm64)",
            },
        ),
        (
            "config.vm.box_url",
            Open {
                note: "custom images: would need a field under `vm:`",
            },
        ),
        ("config.vm.box_check_update", TOOLING),
        ("config.vm.box_download_checksum", TOOLING),
        ("config.vm.hostname", Emitted { from: "the machine's name" }),
        ("config.vm.boot_timeout", Emitted { from: "(fixed: 10 minutes)" }),
        (
            "config.vm.synced_folder",
            Emitted {
                from: "(disabled: the project is copied into each VM instead)",
            },
        ),
        (
            "config.vm.disk",
            Planned {
                note: "machines.*.resources.disk_gb",
            },
        ),
        ("config.vm.guest", Planned { note: "Windows guests" }),
        (
            "config.vm.communicator",
            Planned {
                note: "WinRM, for Windows guests",
            },
        ),
        ("config.vm.cloud_init", VIA_SHELL),
        (
            "config.vm.base_mac",
            ByDesign {
                why: "addresses are fixed at the IP level, the same on every target",
            },
        ),
        (
            "config.vm.base_address",
            ByDesign {
                why: "addresses are fixed at the IP level, the same on every target",
            },
        ),
        ("config.vm.usable_port_range", HOST_SIDE),
        ("config.vm.graceful_halt_timeout", TOOLING),
        ("config.vm.post_up_message", TOOLING),
        ("config.vm.allow_hosts_modification", TOOLING),
        ("config.vm.ignore_box_vagrantfile", TOOLING),
        ("config.ssh", TOOLING),
        ("config.winrm", Planned { note: "with Windows guests" }),
        ("config.trigger", TOOLING),
        // Networks.
        (
            "config.vm.network private_network",
            Emitted {
                from: "networks and machines.*.networks",
            },
        ),
        ("config.vm.network forwarded_port", HOST_SIDE),
        ("config.vm.network public_network", HOST_SIDE),
        // Provisioners.
        (
            "config.vm.provision shell",
            Emitted {
                from: "machines.*.vm.provision (.sh), and Isoloom's own steps",
            },
        ),
        (
            "config.vm.provision file",
            Emitted {
                from: "(the project, copied into each VM)",
            },
        ),
        (
            "config.vm.provision ansible_local",
            Emitted {
                from: "machines.*.vm.provision (.yml, .yaml)",
            },
        ),
        (
            "config.vm.provision ansible",
            ByDesign {
                why: "Ansible runs inside the VM, never on the user's machine",
            },
        ),
        (
            "config.vm.provision docker",
            ByDesign {
                why: "VMs run their services natively; containers come from `docker:`",
            },
        ),
        (
            "config.vm.provision podman",
            ByDesign {
                why: "VMs run their services natively; containers come from `docker:`",
            },
        ),
        ("config.vm.provision puppet", VIA_SHELL),
        ("config.vm.provision chef_solo", VIA_SHELL),
        ("config.vm.provision salt", VIA_SHELL),
        // Providers, and what Isoloom sets on them (cpus, memory, display name).
        (
            "config.vm.provider virtualbox",
            Emitted {
                from: "machines.*.resources (cpus, memory)",
            },
        ),
        (
            "config.vm.provider vmware_desktop",
            Emitted {
                from: "machines.*.resources (cpus, memory)",
            },
        ),
        (
            "config.vm.provider parallels",
            Emitted {
                from: "machines.*.resources (cpus, memory)",
            },
        ),
        (
            "config.vm.provider libvirt",
            Emitted {
                from: "machines.*.resources (cpus, memory)",
            },
        ),
        ("config.vm.provider hyperv", Planned { note: "Hyper-V hosts" }),
        (
            "config.vm.provider docker",
            ByDesign {
                why: "containers come from `docker:` (the Compose output)",
            },
        ),
        ("provider gui", TOOLING),
        (
            "provider linked_clone",
            Open {
                note: "faster starts from one image; no behavior change",
            },
        ),
    ]
}
