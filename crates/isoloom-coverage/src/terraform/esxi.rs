//! ESXi (josenk/esxi), for standalone hosts with no vCenter: its resource types, and the
//! arguments of what the esxi target writes.

use super::support::{ALIAS, DISKS, MTU, PROTECT};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Planned, Tooling};

/// The types the esxi target writes.
const ESXI_WRITTEN: Support = Emitted {
    from: "(written by the esxi target: a VM per machine and the router, a vSwitch and a port group per network)",
};

/// A data disk of its own.
const VIRTUAL_DISK: Support = Equivalent {
    via: "`resources.disk_gb`: the machine's one disk (`boot_disk_size`), which holds its volumes' paths, as on Vagrant's ESXi provider",
};

/// Where the host places a VM.
const PLACEMENT: Support = Tooling {
    note: "where the host places the VMs: no change in behavior",
};

/// How the disk is allocated.
const DISK_FORMAT: Support = Tooling {
    note: "how the host allocates the disk (thin or thick): no change in behavior",
};

/// Another source for the VM.
const SOURCE: Support = Tooling {
    note: "another source for a VM's image (a VM or OVF files already on the host): Isoloom deploys the OVA of `images`",
};

/// Another way to hand cloud-init its documents.
const OVF_PROPERTIES: Support = Tooling {
    note: "another way to hand cloud-init its documents (an OVA's own properties): Isoloom uses `guestinfo`, which every OVA with VMware Tools reads",
};

/// The host's own tuning.
const HOST: Support = NotPortable {
    why: "tunes the ESXi host for the VM or the vSwitch (virtual hardware version, link discovery): no other target exposes it",
};

/// The port group's or vSwitch's security policy.
const SECURITY: Support = NotPortable {
    why: "a vSwitch's security policy (promiscuous mode, MAC changes, forged transmits): no other target's networks have one",
};

/// A network on the host's own uplink.
const UPLINK: Support = Planned {
    note: "with `bridged` networks on ESXi: a network on the host's own uplink (each network is on its own vSwitch with none)",
};

/// The guest OS type, which Windows needs.
const GUEST_OS: Support = Planned {
    note: "with Windows guests on the ESXi target (a Linux OVA keeps its own)",
};

/// A machine's firmware.
const FIRMWARE: Support = Planned {
    note: "with a machine's own boot on ESXi (`vm.boot`), as on Proxmox",
};

/// ESXi: every resource type, and what the generator writes.
pub const TABLE: Table = Table {
    name: "Terraform: ESXi",
    file: ".isoloom/esxi/",
    source: "josenk/esxi 1.10.3 (its resource types, and the schema of what Isoloom writes; not run on a real host)",
    providers: &[Provider {
        name: "esxi",
        types: include_str!("../../coverage/terraform/esxi.txt"),
        schema: include_str!("../../coverage/terraform/esxi-schema.txt"),
    }],
    prefix: "esxi_",
    legacy: None,
    generated: ESXI_WRITTEN,
    types: TYPES,
    written: WRITTEN,
    settings: &[],
    arguments: ARGUMENTS,
    every_argument: None,
    collapse: false,
};

/// Every resource type by family (names without `esxi_`).
const TYPES: Rules = &[(VIRTUAL_DISK, "virtual_disk"), (PLACEMENT, "resource_pool")];

/// What the generator writes, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    ("types", "esxi_guest esxi_portgroup esxi_vswitch"),
    ("provider", "esxi_hostname esxi_hostport esxi_hostssl esxi_password esxi_username"),
    (
        "esxi_guest",
        "boot_disk_size disk_store guest_name guest_startup_timeout guestinfo memsize notes numvcpus ovf_source power
        network_interfaces network_interfaces.mac_address network_interfaces.nic_type network_interfaces.virtual_network",
    ),
    ("esxi_portgroup", "name vlan vswitch"),
    ("esxi_vswitch", "name"),
];

/// The arguments Isoloom leaves out, per resource type.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "esxi_guest",
        &[
            (DISK_FORMAT, "boot_disk_type"),
            (FIRMWARE, "boot_firmware"),
            (SOURCE, "clone_from_vm host_ovf"),
            (PROTECT, "guest_shutdown_timeout ovf_properties_timer"),
            (GUEST_OS, "guestos"),
            (OVF_PROPERTIES, "ovf_properties ovf_properties.*"),
            (PLACEMENT, "resource_pool_name"),
            (HOST, "virthwver"),
            (DISKS, "virtual_disks virtual_disks.*"),
        ],
    ),
    ("esxi_portgroup", &[(SECURITY, "forged_transmits mac_changes promiscuous_mode")]),
    (
        "esxi_vswitch",
        &[
            (SECURITY, "forged_transmits mac_changes promiscuous_mode"),
            (HOST, "link_discovery_mode"),
            (MTU, "mtu"),
            (ALIAS, "ports"),
            (UPLINK, "uplink uplink.*"),
        ],
    ),
];
