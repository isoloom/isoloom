//! The settings of the smaller provider plugins: VirtualBox, VMware Desktop, Parallels, Hyper-V
//! and UTM.

use super::{
    DEVICES, HOST_TUNING, LINKED_CLONE, LINKED_CLONE_ON, MAC, NO_SHARED_FOLDERS, PRIVATE_NETWORKS, PROVIDER_SPECIFIC, RESOURCES, TOOLING, USER_SETUP, VM_NAME,
};
use crate::{
    Support,
    Support::{Emitted, Equivalent, Tooling},
};

pub(super) fn virtualbox(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "name" => VM_NAME,
        "linked_clone" => LINKED_CLONE_ON,
        // Raw `modifyvm` stays out of the format; Isoloom writes the ones it needs.
        "customize" => Emitted {
            from: "machines.*.vm.tpm (a TPM 2.0, `--tpm-type`), machines.*.nested_virtualization (`--nested-hw-virt on`), and the hybrid target's Docker host (its NICs in promiscuous mode, so its containers get their own addresses on the VMs' networks); no spec field writes raw commands",
        },
        "customizations" => PROVIDER_SPECIFIC,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "auto_nat_dns_proxy" | "default_nic_type" => HOST_TUNING,
        "functional_vboxsf" => NO_SHARED_FOLDERS,
        "gui" => DEVICES,
        "check_guest_additions" | "destroy_unused_network_interfaces" | "linked_clone_snapshot" => TOOLING,
        _ => return None,
    })
}

pub(super) fn vmware(s: &str) -> Option<Support> {
    Some(match s {
        "vmx" => Emitted {
            from: "machines.*.resources and machines.*.nested_virtualization: displayName, numvcpus, memsize, and vhv.enable (nested virtualization)",
        },
        "cpus" | "memory" => Equivalent {
            via: "machines.*.resources, written in `vmx`",
        },
        "linked_clone" => LINKED_CLONE,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "nat_device" => HOST_TUNING,
        "base_mac" | "base_address" => MAC,
        "functional_hgfs" | "unmount_default_hgfs" | "shared_folder_special_char" => NO_SHARED_FOLDERS,
        "gui" => DEVICES,
        "utility_host" | "utility_port" | "utility_certificate_path" | "clone_directory" | "force_vmware_license" | "verify_vmnet" => USER_SETUP,
        "allowlist_verified" | "whitelist_verified" | "enable_vmrun_ip_lookup" | "port_forward_network_pause" | "ssh_info_public" => TOOLING,
        _ => return None,
    })
}

pub(super) fn parallels(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "name" => VM_NAME,
        "linked_clone" => LINKED_CLONE_ON,
        "customize" => Emitted {
            from: "machines.*.nested_virtualization (`--nested-virt on`); no spec field writes raw commands",
        },
        "customizations" => PROVIDER_SPECIFIC,
        "network_adapter" | "network_adapters" => PRIVATE_NETWORKS,
        "optimize_power_consumption" => HOST_TUNING,
        "functional_psf" => NO_SHARED_FOLDERS,
        "check_guest_tools" | "update_guest_tools" | "destroy_unused_network_interfaces" | "linked_clone_snapshot" | "regen_src_uuid" => TOOLING,
        _ => return None,
    })
}

pub(super) fn hyperv(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "vmname" => Emitted {
            from: "(the environment and machine names, as the VM's name in Hyper-V)",
        },
        "linked_clone" => LINKED_CLONE_ON,
        "differencing_disk" => LINKED_CLONE,
        "enable_virtualization_extensions" => Emitted {
            from: "machines.*.nested_virtualization",
        },
        "maxmemory" => HOST_TUNING,
        // Isoloom's networks are private switches with an adapter per VM (Vagrant's triggers).
        "vlan_id" => PRIVATE_NETWORKS,
        "mac" => MAC,
        "enable_enhanced_session_mode" => DEVICES,
        "auto_start_action" | "auto_stop_action" | "enable_checkpoints" | "enable_automatic_checkpoints" | "ip_address_timeout" | "vm_integration_services" => {
            TOOLING
        }
        _ => return None,
    })
}

pub(super) fn utm(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "name" => VM_NAME,
        "customize" | "customizations" => PROVIDER_SPECIFIC,
        "directory_share_mode" | "functional_9pfs" => NO_SHARED_FOLDERS,
        "icon" | "notes" => Tooling { note: "how UTM shows the VM" },
        "check_guest_additions" | "wait_time" => TOOLING,
        _ => return None,
    })
}
