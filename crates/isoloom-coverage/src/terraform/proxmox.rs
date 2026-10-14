//! Proxmox (bpg/proxmox): its resource types, and the arguments of what the proxmox target and
//! docker-vm's Proxmox module write.

use super::support::{
    ALIAS, ARCH, CREDENTIALS, FIREWALL, GPU_PASSTHROUGH, MTU, ORDER, PASSTHROUGH_TUNING, PRIVATE_DNS, PROTECT, PROXMOX_IPV6, SERVER, SHARED, TC, TPM, USB,
    VLANS,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Planned, Tooling};

/// The provider's experimental VM resources.
const EXPERIMENTAL: Support = Tooling {
    note: "an experimental VM resource; Isoloom uses proxmox_virtual_environment_vm",
};

/// Linked clones.
const CLONES: Support = Tooling {
    note: "linked clones: faster starts from one image, no change in behavior (as on Hyper-V and ESXi)",
};

/// Resource pools.
const POOLS: Support = Planned {
    note: "with the Proxmox generator",
};

/// SDN subnets.
const SUBNET: Support = Equivalent {
    via: "the router VM, which holds each network's router address (a subnet would put it on the host bridge, which routes between networks)",
};

/// LXC containers.
const LXC: Support = Planned {
    note: "LXC containers: a way to run `docker:` machines on Proxmox",
};

/// Cluster-wide networking.
const MULTI_NODE: Support = Tooling {
    note: "how a cluster spreads networks over its nodes: no change in behavior",
};

/// The server's own network.
const HOST_NETWORK: Support = NotPortable {
    why: "changes the server's own network: no other target has one to change",
};

/// The server itself.
const SERVER_ADMIN: Support = NotPortable {
    why: "administers the Proxmox server itself: no other target has a server to administer",
};

/// The types the Proxmox modules write.
const PROXMOX_WRITTEN: Support = Emitted {
    from: "(written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module)",
};

/// Proxmox: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: Proxmox",
    file: ".isoloom/proxmox/ and .isoloom/docker-vm/proxmox/",
    source: "bpg/proxmox 0.115.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "proxmox",
        types: include_str!("../../coverage/terraform/proxmox.txt"),
        schema: include_str!("../../coverage/terraform/proxmox-schema.txt"),
    }],
    prefix: "proxmox_",
    // Older names of resources that also exist under the short name.
    legacy: Some("proxmox_virtual_environment_"),
    generated: PROXMOX_WRITTEN,
    types: TYPES,
    written: WRITTEN,
    settings: SETTINGS,
    arguments: ARGUMENTS,
    every_argument: None,
    collapse: false,
};

/// Every resource type by family (names without `proxmox_` or `proxmox_virtual_environment_`).
const TYPES: Rules = &[
    (EXPERIMENTAL, "vm vm2"),
    (CLONES, "cloned_vm"),
    (POOLS, "pool pool_membership"),
    (SUBNET, "sdn_subnet"),
    (LXC, "container oci_image"),
    (FIREWALL, "firewall_rules firewall_options firewall_ipset firewall_alias"),
    (MULTI_NODE, "sdn_zone_* sdn_fabric* sdn_controller*"),
    (HOST_NETWORK, "network_*"),
    (
        SERVER_ADMIN,
        "node_firewall cluster_firewall cluster_firewall_security_group acl* acme_* apt_* backup_job* ceph_* cluster_options* ha*
        hardware_mapping_* metrics_server* node_* realm_* replication* storage_* user* certificate* dns* hosts* group* role* time*",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "proxmox_download_file proxmox_sdn_applier proxmox_sdn_vnet proxmox_sdn_zone_simple proxmox_virtual_environment_file
        proxmox_virtual_environment_vm",
    ),
    (
        "provider",
        "api_token endpoint insecure password ssh ssh.agent ssh.node ssh.node.address ssh.node.name ssh.password ssh.private_key
        ssh.username username",
    ),
    ("proxmox_download_file", "content_type datastore_id file_name node_name overwrite_unmanaged url"),
    ("proxmox_sdn_vnet", "id zone"),
    ("proxmox_sdn_zone_simple", "id nodes"),
    (
        "proxmox_virtual_environment_file",
        "content_type datastore_id node_name source_raw source_raw.data source_raw.file_name",
    ),
    (
        "proxmox_virtual_environment_vm",
        "agent agent.enabled bios boot_order clone clone.full clone.vm_id cpu cpu.cores cpu.type disk disk.datastore_id disk.file_id disk.interface
        disk.size efi_disk efi_disk.datastore_id efi_disk.file_format efi_disk.pre_enrolled_keys efi_disk.type initialization initialization.datastore_id initialization.dns
        initialization.dns.servers initialization.ip_config initialization.ip_config.ipv4 initialization.ip_config.ipv4.address
        initialization.ip_config.ipv4.gateway initialization.type initialization.user_data_file_id machine memory memory.dedicated name
        network_device network_device.bridge network_device.mac_address node_name on_boot operating_system operating_system.type
        serial_device tags tpm_state tpm_state.datastore_id tpm_state.version",
    ),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "api_headers auth_ticket csrf_prevention_token min_tls otp tmp_dir ssh.agent_forwarding ssh.agent_socket ssh.node_address_source
        ssh.socks5_password ssh.socks5_server ssh.socks5_username ssh.node.port random_vm_id_end random_vm_id_start random_vm_ids",
)];

/// The arguments Isoloom leaves out, per resource type.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "proxmox_download_file",
        &[(ALIAS, "checksum checksum_algorithm decompression_algorithm overwrite upload_timeout verify")],
    ),
    ("proxmox_sdn_applier", &[(ALIAS, "on_create on_destroy")]),
    ("proxmox_sdn_vnet", &[(ALIAS, "alias"), (SERVER, "isolate_ports"), (VLANS, "tag vlan_aware")]),
    (
        "proxmox_sdn_zone_simple",
        &[(SERVER, "dhcp ipam"), (PRIVATE_DNS, "dns dns_zone reverse_dns"), (MTU, "mtu")],
    ),
    (
        "proxmox_virtual_environment_file",
        &[(
            ALIAS,
            "file_mode overwrite timeout_upload upload_mode source_file source_file.* source_raw.resize",
        )],
    ),
    (
        "proxmox_virtual_environment_vm",
        &[
            (
                PROTECT,
                "delete_unreferenced_disks_on_destroy protection purge_on_destroy reboot reboot_after_update started stop_on_destroy migrate
                template timeout_clone timeout_create timeout_migrate timeout_move_disk timeout_reboot timeout_shutdown_vm timeout_start_vm
                timeout_stop_vm agent.timeout agent.wait_for_ip agent.wait_for_ip.*",
            ),
            (
                ALIAS,
                "vm_id mac_addresses clone clone.* cdrom cdrom.* initialization.file_format initialization.interface initialization.type
                initialization.upgrade initialization.meta_data_file_id initialization.network_data_file_id initialization.vendor_data_file_id
                initialization.user_account initialization.user_account.* disk.file_format disk.import_from disk.path_in_datastore agent.type
                scsi_hardware keyboard_layout tablet_device serial_device.device vga vga.*",
            ),
            (
                SERVER,
                "pool_id hook_script_file_id kvm_arguments cpu.affinity cpu.limit cpu.units cpu.hotplugged hotplug disk.aio disk.backup disk.cache
                disk.discard disk.iothread disk.queues disk.replicate disk.serial disk.ssd disk.speed disk.speed.* memory.floating
                memory.hugepages memory.keep_hugepages memory.shared numa numa.* cpu.numa cpu.sockets agent.trim smbios smbios.* rng rng.*
                watchdog watchdog.* amd_sev amd_sev.* audio_device audio_device.*",
            ),
            (SERVER, "acpi"),
            (ARCH, "cpu.architecture cpu.flags"),
            (TPM, "tpm_state tpm_state.*"),
            (GPU_PASSTHROUGH, "hostpci hostpci.device hostpci.id hostpci.mapping hostpci.pcie"),
            (PASSTHROUGH_TUNING, "hostpci.mdev hostpci.rom_file hostpci.rombar hostpci.xvga"),
            (USB, "usb usb.*"),
            (ORDER, "startup startup.*"),
            (SHARED, "virtiofs virtiofs.*"),
            (PRIVATE_DNS, "initialization.dns.domain"),
            (VLANS, "network_device.vlan_id network_device.trunks"),
            (FIREWALL, "network_device.firewall"),
            (TC, "network_device.rate_limit"),
            (MTU, "network_device.mtu"),
            (SERVER, "network_device.model network_device.queues"),
            (ALIAS, "network_device.disconnected network_device.enabled"),
            (
                PROXMOX_IPV6,
                "initialization.ip_config.ipv6 initialization.ip_config.ipv6.* agent.wait_for_ip.ipv6",
            ),
        ],
    ),
];
