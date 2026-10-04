//! Vagrant: the machine settings (`config.vm.*`), network types and provisioners, then each
//! provider plugin's settings. The setting names come from the pinned upstream config classes
//! (`coverage/vagrant/*.txt`, see SOURCES.md); every one must be classified here, or the
//! coverage tests fail.

use super::{Format, Support, Support::*};

const VM: &str = include_str!("../../coverage/vagrant/vm.txt");
const VIRTUALBOX: &str = include_str!("../../coverage/vagrant/virtualbox.txt");
const HYPERV: &str = include_str!("../../coverage/vagrant/hyperv.txt");
const VMWARE: &str = include_str!("../../coverage/vagrant/vmware_desktop.txt");
const PARALLELS: &str = include_str!("../../coverage/vagrant/parallels.txt");
const LIBVIRT: &str = include_str!("../../coverage/vagrant/libvirt.txt");

/// The setting names in one extracted list (comment lines skipped).
pub fn names(list: &str) -> Vec<&str> {
    list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

const TOOLING: Support = Tooling {
    note: "Vagrant tooling, not the environment's behavior",
};
const USER_SETUP: Support = Tooling {
    note: "the user's own setup (how Vagrant reaches the hypervisor, where it stores things)",
};
const HOST_TUNING: Support = ByDesign {
    why: "hypervisor tuning: no meaning for the same machine as a container or on another provider",
};
const DEVICES: Support = ByDesign {
    why: "display, input and host devices: an environment is reached over its networks",
};
const PROVIDER_SPECIFIC: Support = ByDesign {
    why: "provider-specific commands: the same machine must behave the same on every provider",
};
const NO_SHARED_FOLDERS: Support = Tooling {
    note: "shared folders are off: the project is copied into each VM",
};
const PRIVATE_NETWORKS: Support = ByDesign {
    why: "networks come from `config.vm.network private_network`, the same on every provider",
};
const IMAGE_BOOTS: Support = ByDesign {
    why: "machines boot from their image",
};
const LINKED_CLONE: Support = Planned {
    note: "faster starts from one image (no change in behavior)",
};
const DISK: Support = Planned {
    note: "machines.*.resources.disk_gb",
};
const ARCH: Support = Planned {
    note: "a CPU architecture field (amd64, arm64)",
};
const WINDOWS: Support = Planned { note: "with Windows guests" };
const HYPERV_HOSTS: Support = Planned { note: "with Hyper-V hosts" };
const RESOURCES: Support = Emitted { from: "machines.*.resources" };
const VM_NAME: Support = Emitted {
    from: "(the environment and machine names, as the VM's display name)",
};

pub fn formats() -> Vec<Format> {
    vec![
        core(),
        provider("Vagrant: VirtualBox", "virtualbox", VIRTUALBOX, virtualbox),
        provider("Vagrant: VMware Desktop", "vmware_desktop", VMWARE, vmware),
        provider("Vagrant: Parallels", "parallels", PARALLELS, parallels),
        provider("Vagrant: libvirt", "libvirt", LIBVIRT, libvirt),
        provider("Vagrant: Hyper-V", "hyperv", HYPERV, hyperv),
    ]
}

fn provider(name: &'static str, id: &'static str, list: &'static str, classify: fn(&str) -> Option<Support>) -> Format {
    Format {
        name,
        file: ".isoloom/vagrant/Vagrantfile",
        source: match id {
            "vmware_desktop" => "vagrant-vmware-desktop 3.0.5 (its config class)",
            "parallels" => "vagrant-parallels 2.4.7 (its config class)",
            "libvirt" => "vagrant-libvirt 0.12.2 (its config class)",
            _ => "Vagrant 2.4.9 (the provider's config class)",
        },
        rows: names(list)
            .into_iter()
            .map(|s| (format!("provider {id}: {s}"), classify(s).unwrap_or(Unclassified)))
            .collect(),
    }
}

fn core() -> Format {
    let mut rows: Vec<(String, Support)> = names(VM)
        .into_iter()
        .map(|s| (format!("config.vm.{s}"), machine(s).unwrap_or(Unclassified)))
        .collect();
    let extra: [(&str, Support); 19] = [
        (
            "config.vm.network private_network",
            Emitted {
                from: "networks and machines.*.networks",
            },
        ),
        (
            "config.vm.network forwarded_port",
            ByDesign {
                why: "would expose the environment on the user's machine",
            },
        ),
        (
            "config.vm.network public_network",
            ByDesign {
                why: "would put the environment on the user's LAN",
            },
        ),
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
            "config.vm.provision chef",
            Equivalent {
                via: "a `.sh` provisioning step can run any tool",
            },
        ),
        (
            "config.vm.provision puppet",
            Equivalent {
                via: "a `.sh` provisioning step can run any tool",
            },
        ),
        (
            "config.vm.provision salt",
            Equivalent {
                via: "a `.sh` provisioning step can run any tool",
            },
        ),
        (
            "config.vm.provision cfengine",
            Equivalent {
                via: "a `.sh` provisioning step can run any tool",
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
        (
            "config.vm.provision container",
            ByDesign {
                why: "VMs run their services natively; containers come from `docker:`",
            },
        ),
        ("config.ssh", TOOLING),
        ("config.winrm", WINDOWS),
        ("config.winssh", WINDOWS),
        ("config.trigger", TOOLING),
        ("config.vagrant", TOOLING),
    ];
    rows.extend(extra.into_iter().map(|(k, s)| (k.to_string(), s)));
    Format {
        name: "Vagrant",
        file: ".isoloom/vagrant/Vagrantfile",
        source: "Vagrant 2.4.9 (config.vm, network types, provisioners)",
        rows,
    }
}

fn machine(s: &str) -> Option<Support> {
    Some(match s {
        "define" => Emitted { from: "machines" },
        "box" => Partial {
            from: "machines.*.vm.os",
            gap: "Linux images only; Windows comes later",
        },
        "hostname" => Emitted { from: "the machine's name" },
        "host_name" => Tooling {
            note: "old name of `hostname`",
        },
        "boot_timeout" => Emitted { from: "(fixed: 10 minutes)" },
        "synced_folder" => Emitted {
            from: "(disabled: the project is copied into each VM instead)",
        },
        "network" => Emitted {
            from: "networks (see the network types below)",
        },
        "provision" => Emitted {
            from: "machines.*.vm.provision (see the provisioners below)",
        },
        "provider" => Emitted {
            from: "machines.*.resources (see each provider below)",
        },
        "disk" | "disks" => DISK,
        "box_architecture" => ARCH,
        "box_version" => Planned {
            note: "pinning images to a version",
        },
        "box_url" | "box_server_url" => Open {
            note: "custom images: would need a field under `vm:`",
        },
        "communicator" | "guest" => WINDOWS,
        "cloud_init" | "cloud_init_configs" | "cloud_init_first_boot_only" => Equivalent {
            via: "provisioning steps run any setup",
        },
        "base_mac" | "base_address" => ByDesign {
            why: "addresses are fixed at the IP level, the same on every target",
        },
        "usable_port_range" => ByDesign {
            why: "would expose the environment on the user's machine",
        },
        "provisioners" => Tooling {
            note: "Vagrant's own list behind `provision`",
        },
        "clone" => Tooling {
            note: "starts from another Vagrant machine; machines start from their image",
        },
        "allow_fstab_modification" | "allowed_synced_folder_types" => NO_SHARED_FOLDERS,
        "allow_hosts_modification" => Tooling {
            note: "Isoloom writes /etc/hosts itself (names of the other machines)",
        },
        "post_up_message" | "graceful_halt_timeout" | "ignore_box_vagrantfile" => TOOLING,
        s if s.starts_with("box_") => USER_SETUP,
        _ => return None,
    })
}

fn virtualbox(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "name" => VM_NAME,
        "linked_clone" => LINKED_CLONE,
        "customize" | "customizations" => PROVIDER_SPECIFIC,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "auto_nat_dns_proxy" | "default_nic_type" => HOST_TUNING,
        "functional_vboxsf" => NO_SHARED_FOLDERS,
        "gui" => DEVICES,
        "check_guest_additions" | "destroy_unused_network_interfaces" | "linked_clone_snapshot" => TOOLING,
        _ => return None,
    })
}

fn vmware(s: &str) -> Option<Support> {
    Some(match s {
        "vmx" => Partial {
            from: "machines.*.resources",
            gap: "only displayName, numvcpus and memsize",
        },
        "cpus" | "memory" => Equivalent {
            via: "machines.*.resources, written in `vmx`",
        },
        "linked_clone" => LINKED_CLONE,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "nat_device" => HOST_TUNING,
        "base_mac" | "base_address" => ByDesign {
            why: "addresses are fixed at the IP level, the same on every target",
        },
        "functional_hgfs" | "unmount_default_hgfs" | "shared_folder_special_char" => NO_SHARED_FOLDERS,
        "gui" => DEVICES,
        "utility_host" | "utility_port" | "utility_certificate_path" | "clone_directory" | "force_vmware_license" | "verify_vmnet" => USER_SETUP,
        "allowlist_verified" | "whitelist_verified" | "enable_vmrun_ip_lookup" | "port_forward_network_pause" | "ssh_info_public" => TOOLING,
        _ => return None,
    })
}

fn parallels(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "name" => VM_NAME,
        "linked_clone" => LINKED_CLONE,
        "customize" | "customizations" => PROVIDER_SPECIFIC,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "optimize_power_consumption" => HOST_TUNING,
        "functional_psf" => NO_SHARED_FOLDERS,
        "check_guest_tools" | "update_guest_tools" | "destroy_unused_network_interfaces" | "linked_clone_snapshot" | "regen_src_uuid" => TOOLING,
        _ => return None,
    })
}

fn hyperv(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" | "vmname" => HYPERV_HOSTS,
        "linked_clone" | "differencing_disk" => LINKED_CLONE,
        "maxmemory" | "enable_virtualization_extensions" => HOST_TUNING,
        "vlan_id" | "mac" => PRIVATE_NETWORKS,
        "enable_enhanced_session_mode" => DEVICES,
        "auto_start_action" | "auto_stop_action" | "enable_checkpoints" | "enable_automatic_checkpoints" | "ip_address_timeout" | "vm_integration_services" => {
            TOOLING
        }
        _ => return None,
    })
}

fn libvirt(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "machine_arch" => ARCH,
        "machine_virtual_size" | "storage" | "disks" => DISK,
        "nested" => Open {
            note: "nested virtualization: would need a field under `vm:`",
        },
        "tpm_model" | "tpm_type" | "tpm_path" | "tpm_version" => WINDOWS,
        "boot" | "boot_order" | "kernel" | "cmd_line" | "initrd" | "dtb" | "loader" | "nvram" => IMAGE_BOOTS,
        "uri" | "driver" | "host" | "port" | "connect_via_ssh" | "socket" | "username" | "password" | "id_ssh_key_file" | "proxy_command" | "system_uri"
        | "qemu_use_session" | "storage_pool_name" | "storage_pool_path" | "snapshot_pool_name" | "emulator_path" => USER_SETUP,
        "forward_ssh_port"
        | "random_hostname"
        | "default_prefix"
        | "title"
        | "description"
        | "uuid"
        | "autostart"
        | "suspend_mode"
        | "mgmt_attach"
        | "host_device_exclude_prefixes"
        | "qemu_use_agent"
        | "volume_cache" => TOOLING,
        s if s.starts_with("management_network_") => Tooling {
            note: "Vagrant's management network, used to provision (like the NAT interface elsewhere)",
        },
        s if s.starts_with("graphics_") || s.starts_with("video_") => DEVICES,
        "keymap" | "sound_type" | "input" | "inputs" | "channel" | "channels" | "serial" | "serials" | "usb" | "usbs" | "usb_controller" | "usbctl_dev"
        | "redirdev" | "redirdevs" | "redirfilter" | "redirfilters" | "smartcard" | "smartcard_dev" | "pci" | "pcis" | "rng" | "random" | "watchdog"
        | "watchdog_dev" | "cdroms" | "floppies" => DEVICES,
        s if s.starts_with("cpu") || s.starts_with("clock_") || s.starts_with("memballoon_") || s.starts_with("disk_") => HOST_TUNING,
        "clock_timer"
        | "shares"
        | "nodeset"
        | "numa_nodes"
        | "memory_backing"
        | "memorybacking"
        | "memtune"
        | "memtunes"
        | "features"
        | "features_hyperv"
        | "hyperv_feature"
        | "kvm_hidden"
        | "launchsecurity"
        | "launchsecurity_data"
        | "machine_type"
        | "qemu_args"
        | "qemuargs"
        | "qemu_env"
        | "qemuenv"
        | "sysinfo"
        | "nic_model_type"
        | "nic_adapter_count" => HOST_TUNING,
        _ => return None,
    })
}
