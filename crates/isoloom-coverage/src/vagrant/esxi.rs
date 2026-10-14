//! The settings of vagrant-vmware-esxi, for standalone ESXi hosts.

use super::{DISK, HOST_TUNING, LINKED_CLONE, MAC, MORE_DISKS, TOOLING, USER_SETUP, WINDOWS};
use crate::{
    Support,
    Support::{Emitted, Equivalent, Tooling},
};

pub(super) fn classify(s: &str) -> Option<Support> {
    if let Some(via) = legacy(s) {
        return Some(Equivalent { via });
    }
    Some(match s {
        "guest_numvcpus" | "guest_memsize" => Emitted { from: "machines.*.resources" },
        "guest_name" => Emitted {
            from: "the environment and machine names",
        },
        "esxi_virtual_network" => Emitted {
            from: "networks: a port group per network, from ESXI_VIRTUAL_NETWORK",
        },
        "esxi_hostname" | "esxi_hostport" | "esxi_username" | "esxi_password" | "esxi_disk_store" => Emitted {
            from: "the ESXI_* environment variables (the host is the user's)",
        },
        "guest_boot_disk_size" => DISK,
        "guest_storage" => MORE_DISKS,
        "guest_guestos" | "guestos" => WINDOWS,
        "guest_mac_address" | "mac_address" => MAC,
        "guest_nic_type"
        | "nic_type"
        | "guest_virtualhw_version"
        | "virtualhw_version"
        | "custom_vmx_settings"
        | "guest_custom_vmx_settings"
        | "guest_disk_type"
        | "vm_disk_type" => HOST_TUNING,
        "encoded_esxi_password" | "esxi_private_keys" | "vm_disk_store" | "esxi_resource_pool" | "resource_pool" | "local_private_keys" => USER_SETUP,
        "clone_from_vm" => LINKED_CLONE,
        s if s.starts_with("supported_") => Tooling {
            note: "the plugin's own list of accepted values",
        },
        "allow_overwrite"
        | "local_allow_overwrite"
        | "debug"
        | "lax"
        | "local_lax"
        | "local_failonwarning"
        | "local_use_ip_cache"
        | "guest_autostart"
        | "guest_snapshot_includememory"
        | "guest_snapshot_quiesced"
        | "guest_username"
        | "ssh_username"
        | "saved_ipaddress" => TOOLING,
        _ => return None,
    })
}

/// The plugin's older names, which it copies into the newer ones Isoloom writes (with a warning
/// to migrate), and the name prefix, which only names a VM Vagrant gives no name: Isoloom names
/// every one.
fn legacy(s: &str) -> Option<&'static str> {
    Some(match s {
        "numvcpus" => "guest_numvcpus (the plugin's newer name for it)",
        "memsize" => "guest_memsize (the plugin's newer name for it)",
        "vmname" => "guest_name (the plugin's newer name for it)",
        "virtual_network" => "esxi_virtual_network (the plugin's newer name for it)",
        "guest_name_prefix" | "vmname_prefix" => "guest_name: every VM is named (`<env>-<machine>`), so the plugin never makes up a name with a prefix",
        _ => return None,
    })
}
