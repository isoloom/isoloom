//! DigitalOcean (digitalocean/digitalocean): its resource types by family, and the arguments of
//! what the cloud-vm and cloud-docker targets write.

use super::support::{
    ACCOUNT, ALIAS, BACKUP, CLOUD, CLOUD_NETWORK, CLOUD_WRITTEN, CREDENTIALS, DIGITALOCEAN_IPV6, IMAGES, MANAGED, NAT, OLDER_NAME, PLACEMENT, PROTECT,
    PUBLIC_DNS, PUBLIC_IP, PUBLIC_IPV6,
};
use super::table::{Provider, Rules, Table};

/// An attachment the droplet's `volume_ids` makes.
const ATTACHED_BY_ID: crate::Support = crate::Support::Equivalent {
    via: "machines.*.disks: the `volume_ids` of digitalocean_droplet, each volume attached at creation",
};

/// DigitalOcean: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: DigitalOcean",
    file: ".isoloom/cloud-vm/digitalocean/ and .isoloom/cloud-docker/digitalocean/",
    source: "digitalocean/digitalocean 2.103.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "digitalocean",
        types: include_str!("../../coverage/terraform/digitalocean.txt"),
        schema: include_str!("../../coverage/terraform/digitalocean-schema.txt"),
    }],
    prefix: "digitalocean_",
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
    (PUBLIC_IP, "reserved_ip reserved_ip_assignment"),
    (PUBLIC_IPV6, "reserved_ipv6 reserved_ipv6_assignment"),
    (NAT, "vpc_nat_gateway"),
    (ATTACHED_BY_ID, "volume_attachment"),
    (OLDER_NAME, "floating_ip floating_ip_assignment"),
    (IMAGES, "custom_image"),
    (PUBLIC_DNS, "domain record"),
    (PLACEMENT, "droplet_autoscale"),
    (BACKUP, "droplet_snapshot volume_snapshot nfs_snapshot"),
    (CLOUD_NETWORK, "loadbalancer cdn certificate vpc_peering partner_attachment byoip_prefix"),
    (ACCOUNT, "project project_resources tag monitor_alert uptime_alert uptime_check"),
    (
        MANAGED,
        "agent_platform_* app container_registries container_registry* database_* dedicated_inference* kubernetes_* nfs nfs_access_point nfs_attachment spaces_*
        vector_database",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "digitalocean_volume digitalocean_droplet digitalocean_firewall digitalocean_ssh_key digitalocean_vpc",
    ),
    ("digitalocean_droplet", "volume_ids image name region size ssh_keys tags user_data vpc_uuid"),
    (
        "digitalocean_firewall",
        "droplet_ids inbound_rule inbound_rule.port_range inbound_rule.protocol inbound_rule.source_addresses name outbound_rule
        outbound_rule.destination_addresses outbound_rule.port_range outbound_rule.protocol",
    ),
    ("digitalocean_ssh_key", "name public_key"),
    ("digitalocean_vpc", "ip_range name region"),
    ("digitalocean_volume", "name region size"),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "api_endpoint http_retry_max http_retry_wait_max http_retry_wait_min requests_per_second spaces_access_id spaces_endpoint
        spaces_secret_key token",
)];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "digitalocean_volume",
        &[
            (BACKUP, "snapshot_id"),
            (CLOUD, "filesystem_type initial_filesystem_label initial_filesystem_type"),
            (ALIAS, "description tags"),
        ],
    ),
    (
        "digitalocean_droplet",
        &[
            (CLOUD, "backups backup_policy backup_policy.* droplet_agent monitoring gpu_partition_mode"),
            (PROTECT, "graceful_shutdown resize_disk"),
            (DIGITALOCEAN_IPV6, "ipv6 ipv6_address"),
            (ALIAS, "private_networking public_networking"),
        ],
    ),
    (
        "digitalocean_firewall",
        &[
            (
                ALIAS,
                "inbound_rule.action inbound_rule.source_droplet_ids inbound_rule.source_tags outbound_rule.action
                outbound_rule.destination_droplet_ids outbound_rule.destination_tags",
            ),
            (
                CLOUD,
                "inbound_rule.source_kubernetes_ids inbound_rule.source_load_balancer_uids outbound_rule.destination_kubernetes_ids
                outbound_rule.destination_load_balancer_uids",
            ),
        ],
    ),
];
