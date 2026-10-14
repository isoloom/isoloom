//! Oracle Cloud (oracle/oci): its resource types by family, and the arguments of what the
//! cloud-vm and cloud-docker targets write.

use super::support::{
    ACCOUNT, ADDRESSES, ALIAS, ALIAS_RECORDS, BACKUP, CLOUD, CLOUD_NAMES, CLOUD_NETWORK, CLOUD_RESOLVER, CLOUD_VLANS, CLOUD_WRITTEN, CREDENTIALS, DEFAULTS,
    DISKS, IMAGES, LOCATION, MANAGED, NAT, NESTED, OCI_IPV6, PLACEMENT, PROTECT, PUBLIC_DNS, PUBLIC_IP, SOURCE_PORTS, TAGS, TPM, VM_AGENT, ZONE_SETTINGS,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Equivalent};

/// What a network security group's rule could carry beyond the published ports Isoloom opens
/// with it.
const IN_LIST: Support = Equivalent {
    via: "the network's security list, which carries its egress and ICMP rules (oci_core_security_list)",
};

/// Oracle Cloud: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: Oracle Cloud",
    file: ".isoloom/cloud-vm/oci/ and .isoloom/cloud-docker/oci/",
    source: "oracle/oci 9.8.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "oci",
        types: include_str!("../../coverage/terraform/oci.txt"),
        schema: include_str!("../../coverage/terraform/oci-schema.txt"),
    }],
    prefix: "oci_",
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
    (NAT, "core_nat_gateway"),
    (PUBLIC_IP, "core_public_ip"),
    (ADDRESSES, "core_private_ip"),
    (OCI_IPV6, "core_ipv6"),
    (CLOUD_VLANS, "core_vlan"),
    (BACKUP, "core_boot_volume"),
    (DISKS, "core_volume core_volume_attachment"),
    (CLOUD_RESOLVER, "core_dhcp_options"),
    (IMAGES, "core_image core_compute_image_capability_schema core_shape_management"),
    (ALIAS_RECORDS, "dns_record"),
    (CLOUD_NETWORK, "dns_resolver_endpoint"),
    (
        PUBLIC_DNS,
        "dns_action_create_zone_from_zone_file dns_steering_policy* dns_tsig_key dns_zone_promote_dnssec_key_version
        dns_zone_stage_dnssec_key_version health_checks_*",
    ),
    (DEFAULTS, "core_default_*"),
    (
        PLACEMENT,
        "core_instance_configuration core_instance_pool core_instance_pool_instance core_cluster_network core_compute_capacity_*
        core_compute_cluster core_compute_gpu_memory_* core_compute_host core_compute_host_group core_dedicated_vm_host
        core_instance_maintenance_event capacity_management_* autoscaling_* cluster_placement_groups_* compute_cloud_at_customer_*
        resource_scheduler_*",
    ),
    (
        BACKUP,
        "core_boot_volume_backup core_volume_backup* core_volume_group core_volume_group_backup disaster_recovery_* recovery_*",
    ),
    (
        VM_AGENT,
        "core_instance_console_connection core_console_history os_management_hub_* management_agent_* osp_*",
    ),
    (
        CLOUD_NETWORK,
        "core_byoasn core_capture_filter core_cpe core_cross_connect* core_drg* core_ipsec* core_local_peering_gateway
        core_remote_peering_connection core_service_gateway core_virtual_circuit core_vtap core_public_ip_pool* load_balancer*
        network_load_balancer_* network_firewall_* waf_* waas_* bastion_* psa_* resourcemanager_private_endpoint vn_monitoring_*
        core_virtual_network certificates_management_* waa_*",
    ),
    (
        ACCOUNT,
        "identity_* limits_* budget_* announcements_* audit_* logging_* monitoring_* ons_* events_* metering_* optimizer_* usage_*
        license_manager_* marketplace_* core_app_catalog_* core_listing_resource_version_agreement tenantmanagercontrolplane_* self_*
        delegate_* operator_* zpr_* cloud_guard_* resource_analytics_* management_dashboard_* sch_* apiaccesscontrol_* costad_*
        demand_signal_* stack_monitoring_* log_analytics_* security_attribute_* service_catalog_* apm_* cloud_bridge_* cloud_migrations_*",
    ),
    (
        MANAGED,
        "adm_* ai_* analytics_* api_* apigateway_* appmgmt_* artifacts_* batch_* bds_* blockchain_* cluster_health_* container_*
        containerengine_* data_* database_* datacatalog_* datacc_* dataflow_* dataintegration_* datascience_* dblm_* dbmulticloud_* ddfs_*
        desktops_* devops_* dif_* email_* file_* fleet_* functions_* fusion_* gdp_* generative_* generic_* golden_* integration_* iot_*
        jms_* kms_* lustre_* managed_* media_* mysql_* nosql_* objectstorage_* oce_* ocvp_* oda_* opa_* opensearch_* opsi_* psql_* queue_*
        redis_* streaming_* vault_* vbs_* visual_* vulnerability_*",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "data.oci_core_images data.oci_core_private_ips data.oci_core_vcn_dns_resolver_association data.oci_core_vnic
        data.oci_identity_availability_domains oci_core_instance oci_core_internet_gateway oci_core_network_security_group
        oci_core_network_security_group_security_rule oci_core_route_table oci_core_route_table_attachment oci_core_security_list
        oci_core_subnet oci_core_vcn oci_core_vnic_attachment oci_dns_resolver oci_dns_rrset oci_dns_view oci_dns_zone",
    ),
    ("provider", "region"),
    (
        "data.oci_core_images",
        "compartment_id operating_system operating_system_version shape sort_by sort_order",
    ),
    ("data.oci_core_private_ips", "ip_address subnet_id"),
    ("data.oci_core_vcn_dns_resolver_association", "vcn_id"),
    ("data.oci_core_vnic", "vnic_id"),
    ("data.oci_identity_availability_domains", "compartment_id"),
    ("oci_dns_resolver", "attached_views attached_views.view_id resolver_id scope"),
    (
        "oci_dns_rrset",
        "domain items items.domain items.rdata items.rtype items.ttl rtype zone_name_or_id",
    ),
    ("oci_dns_view", "compartment_id display_name scope"),
    ("oci_dns_zone", "compartment_id name scope view_id zone_type"),
    (
        "oci_core_instance",
        "availability_domain compartment_id create_vnic_details create_vnic_details.assign_public_ip create_vnic_details.hostname_label
        create_vnic_details.nsg_ids create_vnic_details.private_ip create_vnic_details.skip_source_dest_check create_vnic_details.subnet_id
        display_name metadata shape shape_config shape_config.memory_in_gbs
        platform_config platform_config.is_measured_boot_enabled platform_config.is_secure_boot_enabled
        platform_config.is_trusted_platform_module_enabled platform_config.type shape_config.ocpus source_details
        source_details.boot_volume_size_in_gbs source_details.source_id source_details.source_type",
    ),
    ("oci_core_internet_gateway", "compartment_id display_name vcn_id"),
    ("oci_core_network_security_group", "compartment_id display_name vcn_id"),
    (
        "oci_core_network_security_group_security_rule",
        "description direction network_security_group_id protocol source source_type tcp_options tcp_options.destination_port_range
        tcp_options.destination_port_range.max tcp_options.destination_port_range.min udp_options udp_options.destination_port_range
        udp_options.destination_port_range.max udp_options.destination_port_range.min",
    ),
    (
        "oci_core_route_table",
        "compartment_id display_name route_rules route_rules.destination route_rules.network_entity_id vcn_id",
    ),
    (
        "oci_core_security_list",
        "compartment_id display_name egress_security_rules egress_security_rules.description egress_security_rules.destination
        egress_security_rules.icmp_options egress_security_rules.icmp_options.code egress_security_rules.icmp_options.type
        egress_security_rules.protocol egress_security_rules.tcp_options egress_security_rules.tcp_options.max
        egress_security_rules.tcp_options.min egress_security_rules.udp_options egress_security_rules.udp_options.max
        egress_security_rules.udp_options.min ingress_security_rules ingress_security_rules.icmp_options ingress_security_rules.icmp_options.code
        ingress_security_rules.icmp_options.type ingress_security_rules.description ingress_security_rules.protocol ingress_security_rules.source
        ingress_security_rules.tcp_options ingress_security_rules.tcp_options.max ingress_security_rules.tcp_options.min
        ingress_security_rules.udp_options ingress_security_rules.udp_options.max ingress_security_rules.udp_options.min vcn_id",
    ),
    ("oci_core_route_table_attachment", "route_table_id subnet_id"),
    (
        "oci_core_subnet",
        "cidr_block compartment_id display_name route_table_id security_list_ids vcn_id",
    ),
    ("oci_core_vcn", "cidr_block cidr_blocks compartment_id display_name"),
    (
        "oci_core_vnic_attachment",
        "create_vnic_details create_vnic_details.assign_public_ip create_vnic_details.private_ip create_vnic_details.skip_source_dest_check
        create_vnic_details.subnet_id display_name instance_id",
    ),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "auth config_file_profile disable_auto_retries dual_stack_endpoint_enabled fingerprint ignore_defined_tags private_key
        private_key_password private_key_path realm_specific_service_endpoint_template_enabled retries_config_file retry_duration_seconds
        tenancy_ocid test_time_maintenance_reboot_due token_exchange_auth token_exchange_client_id token_exchange_client_secret
        token_exchange_domain_url token_exchange_public_key token_exchange_requested_token_type token_exchange_resource_type
        token_exchange_rpst_exp token_exchange_subject_token_type user_ocid workload_identity_token_path",
)];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    ("oci_dns_resolver", &[(ALIAS, "compartment_id"), (CLOUD_NETWORK, "rules.*")]),
    ("oci_dns_rrset", &[(ALIAS, "compartment_id scope view_id")]),
    (
        "oci_dns_zone",
        &[
            (ZONE_SETTINGS, "dnssec_state resolution_mode"),
            (CLOUD_NETWORK, "external_downstreams.* external_masters.*"),
        ],
    ),
    (
        "oci_core_instance",
        &[
            (
                PROTECT,
                "async preserve_boot_volume preserve_data_volumes_created_at_launch state update_operation_constraint
                source_details.is_preserve_boot_volume_enabled",
            ),
            (
                PLACEMENT,
                "capacity_reservation_id cluster_placement_group_id compute_cluster_id dedicated_vm_host_id fault_domain instance_configuration_id
                placement_constraint_details placement_constraint_details.* preemptible_instance_config preemptible_instance_config.*
                licensing_configs licensing_configs.* availability_config availability_config.*",
            ),
            (
                CLOUD,
                "extended_metadata ipxe_script is_ai_enterprise_enabled is_pv_encryption_in_transit_enabled security_attributes agent_config
                agent_config.* instance_options instance_options.* launch_options launch_options.* platform_config.*
                shape_config.baseline_ocpu_utilization shape_config.local_volume_size_in_gbs shape_config.nvmes shape_config.resource_management
                source_details.boot_volume_vpus_per_gb source_details.kms_key_id create_vnic_details.security_attributes",
            ),
            (
                TPM,
                "platform_config.is_secure_boot_enabled platform_config.is_trusted_platform_module_enabled platform_config.is_measured_boot_enabled",
            ),
            (NESTED, "platform_config platform_config.type platform_config.are_virtual_instructions_enabled"),
            (
                ALIAS,
                "hostname_label image subnet_id shape_config.vcpus source_details.instance_source_image_filter_details
                source_details.instance_source_image_filter_details.* create_vnic_details.private_ip_id create_vnic_details.subnet_cidr",
            ),
            (
                OCI_IPV6,
                "create_vnic_details.assign_ipv6ip create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details
                create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details.*",
            ),
            (CLOUD_NAMES, "create_vnic_details.assign_private_dns_record"),
            (CLOUD_VLANS, "create_vnic_details.vlan_id"),
            (DISKS, "launch_volume_attachments launch_volume_attachments.*"),
        ],
    ),
    ("oci_core_internet_gateway", &[(PROTECT, "enabled"), (CLOUD_NETWORK, "route_table_id")]),
    (
        "oci_core_network_security_group_security_rule",
        &[
            (CLOUD, "stateless"),
            (IN_LIST, "destination destination_type icmp_options icmp_options.*"),
            (
                SOURCE_PORTS,
                "tcp_options.source_port_range tcp_options.source_port_range.* udp_options.source_port_range udp_options.source_port_range.*",
            ),
        ],
    ),
    (
        "oci_core_route_table",
        &[
            (ALIAS, "route_rules.cidr_block route_rules.destination_type route_rules.route_type"),
            (TAGS, "route_rules.description"),
        ],
    ),
    (
        "oci_core_security_list",
        &[
            (ALIAS, "egress_security_rules.destination_type ingress_security_rules.source_type"),
            (CLOUD, "egress_security_rules.stateless ingress_security_rules.stateless"),
            (
                SOURCE_PORTS,
                "ingress_security_rules.tcp_options.source_port_range ingress_security_rules.tcp_options.source_port_range.*
                ingress_security_rules.udp_options.source_port_range ingress_security_rules.udp_options.source_port_range.*
                egress_security_rules.tcp_options.source_port_range egress_security_rules.tcp_options.source_port_range.*
                egress_security_rules.udp_options.source_port_range egress_security_rules.udp_options.source_port_range.*",
            ),
        ],
    ),
    (
        "oci_core_subnet",
        &[
            (LOCATION, "availability_domain"),
            (CLOUD_NAMES, "dns_label"),
            (CLOUD_RESOLVER, "dhcp_options_id"),
            (ALIAS, "ipv4cidr_blocks"),
            (OCI_IPV6, "ipv6cidr_block ipv6cidr_blocks"),
            (ALIAS, "prohibit_internet_ingress prohibit_public_ip_on_vnic"),
        ],
    ),
    (
        "oci_core_vcn",
        &[
            (CLOUD_NAMES, "dns_label"),
            (
                OCI_IPV6,
                "ipv6private_cidr_blocks is_ipv6enabled is_oracle_gua_allocation_enabled byoipv6cidr_details byoipv6cidr_details.*",
            ),
            (CLOUD, "security_attributes"),
        ],
    ),
    (
        "oci_core_vnic_attachment",
        &[
            (CLOUD, "nic_index create_vnic_details.security_attributes"),
            (
                ALIAS,
                "create_vnic_details.hostname_label create_vnic_details.nsg_ids create_vnic_details.private_ip_id
                create_vnic_details.subnet_cidr create_vnic_details.route_table_id",
            ),
            (CLOUD_NAMES, "create_vnic_details.assign_private_dns_record"),
            (CLOUD_VLANS, "create_vnic_details.vlan_id"),
            (
                OCI_IPV6,
                "create_vnic_details.assign_ipv6ip create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details
                create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details.*",
            ),
        ],
    ),
    ("data.oci_core_private_ips", &[(ALIAS, "ip_state lifetime vlan_id vnic_id filter filter.*")]),
    ("data.oci_core_images", &[(ALIAS, "state filter filter.*")]),
    ("data.oci_identity_availability_domains", &[(LOCATION, "filter filter.*")]),
];
