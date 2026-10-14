//! Proxmox (bpg/proxmox): its resource types, and the arguments of what the proxmox target and
//! docker-vm's Proxmox module write.

use super::support::{
    ALIAS, CREDENTIALS, GPU_PASSTHROUGH, ID, LOCATION, ORDER, PASSTHROUGH_TUNING, PRIVATE_DNS, PROTECT, PROXMOX_IPV6, SERVER, SHARED, SOURCE_PORTS, TAGS, TPM,
    USB,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Tooling};

/// The provider's experimental VM resources.
const EXPERIMENTAL: Support = Tooling {
    note: "an experimental VM resource; Isoloom uses proxmox_virtual_environment_vm",
};

/// Linked clones.
const CLONES: Support = Tooling {
    note: "linked clones: faster starts from one image, no change in behavior (as on Hyper-V and ESXi)",
};

/// A VM's or container's pool, set on it.
const MEMBERSHIP: Support = Tooling {
    note: "another way to put a VM or a container in a pool: Isoloom sets each one's `pool_id`",
};

/// Named addresses for firewall rules.
const FIREWALL_ALIAS: Support = Tooling {
    note: "a name for an address: the firewall rules name each network's IP set instead",
};

/// What an LXC container doesn't carry (`docker.lxc` refuses a machine that asks for it).
const LXC_REFUSED: Support = NotPortable {
    why: "what an LXC container can't carry the way Docker does (mount points, devices, its own user mapping): a `docker.lxc` machine that asks for it is refused, so give it a `vm:`",
};

/// Firewall settings Isoloom leaves at Proxmox's defaults.
const FIREWALL_DEFAULTS: Support = Tooling {
    note: "a firewall setting left at Proxmox's default (logging, DHCP, MAC and IP filters, NDP, router advertisements): no change in what the reach rules allow",
};

/// The firewall rule fields Isoloom has no use for.
const RULE_FIELDS: Support = Tooling {
    note: "another way to write a rule (an interface, a security group, logging, disabled rules): Isoloom names networks by IP set",
};

/// SDN subnets.
const SUBNET: Support = Equivalent {
    via: "the router VM, which holds each network's router address (a subnet would put it on the host bridge, which routes between networks)",
};

/// A VLAN tag on the VNet itself.
const VNET_TAG: Support = Equivalent {
    via: "`network_device.vlan_id` on the LAN's VLAN-aware VNet (`vlan_aware`): a VNet's own tag needs a VLAN zone, the environment's is a simple one",
};

/// Trunks on one device.
const TRUNKS: Support = Equivalent {
    via: "a tagged device per VLAN (`network_device.vlan_id`), the VM's trunk built inside it (a veth, `<lan>.<id>`) as on the other VM targets",
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
    (MEMBERSHIP, "pool_membership"),
    (SUBNET, "sdn_subnet"),
    (FIREWALL_ALIAS, "firewall_alias"),
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
        "proxmox_download_file proxmox_oci_image proxmox_sdn_applier proxmox_sdn_vnet proxmox_sdn_zone_simple
        proxmox_virtual_environment_container proxmox_virtual_environment_file proxmox_virtual_environment_firewall_ipset
        proxmox_virtual_environment_firewall_options proxmox_virtual_environment_firewall_rules proxmox_virtual_environment_pool
        proxmox_virtual_environment_vm",
    ),
    (
        "provider",
        "api_token endpoint insecure password ssh ssh.agent ssh.node ssh.node.address ssh.node.name ssh.password ssh.private_key
        ssh.username username",
    ),
    ("proxmox_download_file", "content_type datastore_id file_name node_name overwrite_unmanaged url"),
    ("proxmox_oci_image", "datastore_id file_name node_name overwrite_unmanaged reference"),
    ("proxmox_sdn_vnet", "id vlan_aware zone"),
    ("proxmox_sdn_zone_simple", "id mtu nodes"),
    (
        "proxmox_virtual_environment_container",
        "cpu cpu.cores disk disk.datastore_id disk.size environment_variables initialization initialization.dns initialization.dns.servers
        initialization.entrypoint initialization.hostname initialization.ip_config initialization.ip_config.ipv4
        initialization.ip_config.ipv4.address initialization.ip_config.ipv4.gateway memory memory.dedicated network_interface
        network_interface.bridge network_interface.firewall network_interface.mtu network_interface.name network_interface.rate_limit
        network_interface.vlan_id node_name operating_system operating_system.template_file_id pool_id start_on_boot tags unprivileged",
    ),
    (
        "proxmox_virtual_environment_firewall_ipset",
        "cidr cidr.name comment name",
    ),
    (
        "proxmox_virtual_environment_firewall_options",
        "container_id enabled input_policy node_name output_policy vm_id",
    ),
    (
        "proxmox_virtual_environment_firewall_rules",
        "container_id node_name rule rule.action rule.comment rule.dest rule.dport rule.macro rule.proto rule.source rule.type vm_id",
    ),
    ("proxmox_virtual_environment_pool", "comment pool_id"),
    (
        "proxmox_virtual_environment_file",
        "content_type datastore_id node_name source_raw source_raw.data source_raw.file_name",
    ),
    (
        "proxmox_virtual_environment_vm",
        "agent agent.enabled bios boot_order clone clone.full clone.vm_id cpu cpu.architecture cpu.cores cpu.type disk disk.datastore_id disk.file_id disk.interface
        disk.size efi_disk efi_disk.datastore_id efi_disk.file_format efi_disk.pre_enrolled_keys efi_disk.type initialization initialization.datastore_id initialization.dns
        initialization.dns.servers initialization.ip_config initialization.ip_config.ipv4 initialization.ip_config.ipv4.address
        initialization.ip_config.ipv4.gateway initialization.interface initialization.type initialization.user_data_file_id machine memory memory.dedicated name
        network_device network_device.bridge network_device.firewall network_device.mac_address network_device.mtu
        network_device.rate_limit network_device.vlan_id node_name on_boot operating_system operating_system.type pool_id
        serial_device tags vga vga.type tpm_state tpm_state.datastore_id tpm_state.version",
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
    ("proxmox_oci_image", &[(ALIAS, "overwrite"), (PROTECT, "upload_timeout")]),
    ("proxmox_virtual_environment_pool", &[(ID, "id")]),
    (
        "proxmox_virtual_environment_firewall_ipset",
        &[
            (ID, "id"),
            (LOCATION, "container_id node_name vm_id"),
            (ALIAS, "cidr.comment"),
            (RULE_FIELDS, "cidr.nomatch"),
        ],
    ),
    (
        "proxmox_virtual_environment_firewall_options",
        &[(ID, "id"), (FIREWALL_DEFAULTS, "dhcp ipfilter log_level_in log_level_out macfilter ndp radv")],
    ),
    (
        "proxmox_virtual_environment_firewall_rules",
        &[
            (ID, "id"),
            (RULE_FIELDS, "rule.enabled rule.iface rule.log rule.security_group"),
            (SOURCE_PORTS, "rule.sport"),
        ],
    ),
    (
        "proxmox_virtual_environment_container",
        &[
            (
                PROTECT,
                "delete_unreferenced_disks_on_destroy protection purge_on_destroy started template timeout_clone timeout_create timeout_delete
                timeout_start timeout_update wait_for_ip wait_for_ip.*",
            ),
            (TAGS, "description"),
            (
                ALIAS,
                "id vm_id clone clone.* console console.* operating_system.type initialization.dns.server initialization.user_account
                initialization.user_account.* network_interface.enabled network_interface.mac_address",
            ),
            (
                SERVER,
                "hook_script_file_id cpu.limit cpu.units disk.acl disk.mount_options disk.quota disk.replicate memory.swap features features.*
                network_interface.host_managed",
            ),
            (ORDER, "startup startup.*"),
            (PRIVATE_DNS, "initialization.dns.domain"),
            (
                LXC_REFUSED,
                "cpu.architecture device_passthrough device_passthrough.* idmap idmap.* mount_point mount_point.* initialization.ip_config.ipv6
                initialization.ip_config.ipv6.*",
            ),
        ],
    ),
    ("proxmox_sdn_vnet", &[(ALIAS, "alias"), (SERVER, "isolate_ports"), (VNET_TAG, "tag")]),
    ("proxmox_sdn_zone_simple", &[(SERVER, "dhcp ipam"), (PRIVATE_DNS, "dns dns_zone reverse_dns")]),
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
                "vm_id mac_addresses clone clone.* cdrom cdrom.* initialization.file_format initialization.type
                initialization.upgrade initialization.meta_data_file_id initialization.network_data_file_id initialization.vendor_data_file_id
                initialization.user_account initialization.user_account.* disk.file_format disk.import_from disk.path_in_datastore agent.type
                scsi_hardware keyboard_layout tablet_device serial_device.device vga.clipboard vga.memory",
            ),
            (
                SERVER,
                "hook_script_file_id kvm_arguments cpu.affinity cpu.limit cpu.units cpu.hotplugged hotplug disk.aio disk.backup disk.cache
                disk.discard disk.iothread disk.queues disk.replicate disk.serial disk.ssd disk.speed disk.speed.* memory.floating
                memory.hugepages memory.keep_hugepages memory.shared numa numa.* cpu.numa cpu.sockets agent.trim smbios smbios.* rng rng.*
                watchdog watchdog.* amd_sev amd_sev.* audio_device audio_device.*",
            ),
            (SERVER, "acpi"),
            (SERVER, "cpu.flags"),
            (TPM, "tpm_state tpm_state.*"),
            (GPU_PASSTHROUGH, "hostpci hostpci.device hostpci.id hostpci.mapping hostpci.pcie"),
            (PASSTHROUGH_TUNING, "hostpci.mdev hostpci.rom_file hostpci.rombar hostpci.xvga"),
            (USB, "usb usb.*"),
            (ORDER, "startup startup.*"),
            (SHARED, "virtiofs virtiofs.*"),
            (PRIVATE_DNS, "initialization.dns.domain"),
            (TRUNKS, "network_device.trunks"),
            (SERVER, "network_device.model network_device.queues"),
            (ALIAS, "network_device.disconnected network_device.enabled"),
            (
                PROXMOX_IPV6,
                "initialization.ip_config.ipv6 initialization.ip_config.ipv6.* agent.wait_for_ip.ipv6",
            ),
        ],
    ),
];
