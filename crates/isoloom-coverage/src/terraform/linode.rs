//! Linode (linode/linode): its resource types by family, and the arguments of what the
//! cloud-vm and cloud-docker targets write.

use super::support::{
    ACCOUNT, ADDRESSES, ALIAS, CLOUD, CLOUD_NETWORK, CLOUD_WRITTEN, CREDENTIALS, EGRESS_RULES, ENVIRONMENT, IMAGES, LINODE_IPV6, MANAGED, PLACEMENT, PROTECT,
    PUBLIC_DNS, VM_AGENT,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Equivalent};

/// Keys stored in the account.
const SSH_KEY: Support = Equivalent {
    via: "each instance's own `authorized_keys`, which installs the environment's key without storing it in the account",
};

/// Linode: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: Linode",
    file: ".isoloom/cloud-vm/linode/ and .isoloom/cloud-docker/linode/",
    source: "linode/linode 4.7.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "linode",
        types: include_str!("../../coverage/terraform/linode.txt"),
        schema: include_str!("../../coverage/terraform/linode-schema.txt"),
    }],
    prefix: "linode_",
    legacy: None,
    generated: CLOUD_WRITTEN,
    types: TYPES,
    written: WRITTEN,
    settings: SETTINGS,
    arguments: ARGUMENTS,
    every_argument: None,
    collapse: true,
};

/// Every resource type by family (names without the provider's prefix).
const TYPES: Rules = &[
    (
        ENVIRONMENT,
        "instance_config instance_disk instance_ip firewall_device volume interface reserved_ip reserved_ip_assignment networking_ip
        networking_ip_assignment ipv6_range instance_shared_ips",
    ),
    (
        IMAGES,
        "image consumer_image_share_group_token producer_image_share_group producer_image_share_group_member",
    ),
    (PUBLIC_DNS, "domain domain_record rdns"),
    (PLACEMENT, "placement_group placement_group_assignment"),
    (VM_AGENT, "stackscript"),
    (CLOUD_NETWORK, "nodebalancer nodebalancer_config nodebalancer_node"),
    (ACCOUNT, "account_settings firewall_settings iam_user user token lock monitor_*"),
    (MANAGED, "database_* lke_* object_storage_*"),
    (SSH_KEY, "sshkey"),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    ("types", "linode_firewall linode_instance linode_vpc linode_vpc_subnet"),
    (
        "linode_firewall",
        "inbound inbound.action inbound.ipv4 inbound.label inbound.ports inbound.protocol inbound_policy label linodes outbound_policy",
    ),
    (
        "linode_instance",
        "authorized_keys booted image interface interface.ipv4 interface.ipv4.vpc interface.purpose interface.subnet_id label metadata
        metadata.user_data region root_pass tags type",
    ),
    ("linode_vpc", "label region"),
    ("linode_vpc_subnet", "ipv4 label vpc_id"),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "api_ca_path api_version config_path config_profile disable_internal_cache event_poll_ms lke_event_poll_ms lke_node_ready_poll_ms
        max_retry_delay_ms min_retry_delay_ms obj_access_key obj_bucket_force_delete obj_secret_key obj_use_temp_keys
        skip_implicit_reboots skip_instance_delete_poll skip_instance_ready_poll skip_lke_cluster_delete_poll token ua_prefix url",
)];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "linode_firewall",
        &[
            (PROTECT, "disabled"),
            (ALIAS, "interfaces"),
            (CLOUD, "nodebalancers"),
            (LINODE_IPV6, "inbound.ipv6 outbound.ipv6"),
            (
                EGRESS_RULES,
                "outbound outbound.action outbound.description outbound.ipv4 outbound.label outbound.ports outbound.protocol",
            ),
        ],
    ),
    (
        "linode_instance",
        &[
            (
                CLOUD,
                "backup_id backups_enabled disk_encryption maintenance_policy migration_type watchdog_enabled alerts alerts.* kernel network_helper",
            ),
            (PROTECT, "resize_disk"),
            (PLACEMENT, "placement_group placement_group.* placement_group_externally_managed"),
            (
                ALIAS,
                "authorized_users boot_config_label boot_size stackscript_data stackscript_id swap_size interface_generation ipv4 shared_ipv4
                firewall_id config config.* disk disk.* linode_interfaces linode_interfaces.* interface.label interface.primary
                interface.ipam_address interface.ip_ranges interface.ipv4.nat_1_1",
            ),
            (ADDRESSES, "private_ip"),
            (LINODE_IPV6, "interface.ipv6 interface.ipv6.*"),
        ],
    ),
    ("linode_vpc", &[(LINODE_IPV6, "ipv6 ipv6.*"), (ALIAS, "ipv4 ipv4.range"), (CLOUD, "vpc_type")]),
    ("linode_vpc_subnet", &[(LINODE_IPV6, "ipv6 ipv6.*")]),
];
