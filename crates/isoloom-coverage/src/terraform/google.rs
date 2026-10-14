//! Google Cloud (hashicorp/google): its resource types by family, and the arguments of what
//! the cloud-vm and cloud-docker targets write.

use super::support::{
    ACCELERATORS, ACCOUNT, ADDRESSES, ALIAS, BACKUP, CLOUD, CLOUD_NETWORK, CLOUD_WRITTEN, CPUS, CREDENTIALS, DISKS, GCP_IPV6, GPU_SCHEDULING, IMAGES, LOCATION,
    MANAGED, MONITORING, MTU, NAT, NESTED, PLACEMENT, PRIVATE_DNS, PROTECT, PUBLIC_DNS, ROUTER, TAGS, TPM, VLANS, VM_AGENT,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Equivalent};

/// Firewall and key resources whose effect the firewall rules and instance metadata give.
const INLINE: Support = Equivalent {
    via: "google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys",
};

/// A reserved address.
const ADDRESS: Support = Equivalent {
    via: "`network_interface.network_ip` of google_compute_instance, which fixes each machine's spec address (its public one is Isoloom's way in, as on every cloud)",
};

/// Google Cloud: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: Google Cloud",
    file: ".isoloom/cloud-vm/gcp/ and .isoloom/cloud-docker/gcp/",
    source: "hashicorp/google 8.5.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "google",
        types: include_str!("../../coverage/terraform/google.txt"),
        schema: include_str!("../../coverage/terraform/google-schema.txt"),
    }],
    prefix: "google_",
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
    (ADDRESS, "compute_address"),
    (NAT, "compute_router compute_router_nat compute_router_nat_address"),
    (ROUTER, "compute_route"),
    (DISKS, "compute_disk compute_attached_disk"),
    (
        INLINE,
        "compute_network_firewall_policy* compute_region_network_firewall_policy* os_login_ssh_public_key",
    ),
    (IMAGES, "compute_image"),
    (PRIVATE_DNS, "dns_managed_zone dns_record_set dns_policy"),
    (PUBLIC_DNS, "clouddomains_* site_verification_*"),
    (
        PLACEMENT,
        "compute_instance_template compute_region_instance_template compute_instance_from_template compute_instance_group*
        compute_region_instance_group_manager compute_autoscaler compute_region_autoscaler compute_reservation compute_region_commitment
        compute_node_group compute_node_template compute_resource_policy compute_resource_policy_attachment
        compute_disk_resource_policy_attachment compute_region_disk_resource_policy_attachment compute_per_instance_config
        compute_region_per_instance_config compute_bulk_per_instance_config compute_resize_request compute_region_resize_request
        compute_instance_settings compute_storage_pool compute_rollout_plan compute_region_disk",
    ),
    (
        BACKUP,
        "compute_snapshot compute_snapshot_settings compute_instant_snapshot compute_region_instant_snapshot compute_disk_async_replication
        backup_*",
    ),
    (VM_AGENT, "compute_global_vm_extension_policy compute_zone_vm_extension_policy os_config_*"),
    (
        CLOUD_NETWORK,
        "compute_backend_* compute_region_backend_service compute_forwarding_rule compute_global_* compute_health_check
        compute_http_health_check compute_https_health_check compute_region_health_* compute_region_composite_health_check
        compute_region_health_check compute_target_* compute_region_target_* compute_url_map compute_region_url_map compute_ssl_*
        compute_region_ssl_* compute_security_policy* compute_region_security_policy* compute_interconnect* compute_vpn_*
        compute_ha_vpn_gateway compute_external_vpn_gateway compute_router_peer compute_router_interface compute_router_named_set
        compute_router_route_policy compute_network_peering* compute_network_endpoint* compute_region_network_endpoint*
        compute_network_attachment compute_service_attachment compute_packet_mirroring compute_public_* compute_managed_ssl_certificate
        compute_cross_site_network compute_wire_group compute_network_edge_security_service network_connectivity_* network_services_*
        network_security_* service_networking_* vpc_access_connector dns_response_policy* cloud_ids_* certificate_manager_*
        service_directory_* iap_*",
    ),
    (
        ACCOUNT,
        "compute_disk_iam_* compute_image_iam_* compute_instance_iam_* compute_instance_template_iam_* compute_instant_snapshot_iam_*
        compute_region_disk_iam_* compute_region_instant_snapshot_iam_* compute_snapshot_iam_* compute_storage_pool_iam_*
        compute_subnetwork_iam_* compute_network_firewall_policy_iam_* compute_region_network_firewall_policy_iam_*
        compute_firewall_policy* compute_organization_security_policy* compute_project_* compute_preview_feature compute_shared_vpc_*
        dns_managed_zone_iam_* project_access_approval_settings project_default_service_accounts project_iam_* project_organization_policy
        project_usage_export_bucket folder folder_* organization_* billing_* iam_* logging_* monitoring_* tags_* resource_manager_*
        service_account service_account_* cloud_asset_* cloud_identity_* cloud_quotas_* cloud_support_* essential_*
        access_context_manager_* org_* privileged_* assured_* cloud_security_compliance_* network_management_* apikeys_* observability_*
        contact_* workload_identity_*",
    ),
    (
        MANAGED,
        "active_directory_* agent_* agentic_* alloydb_* apigee_* apihub_* app_engine_* apphub_* artifact_* beyondcorp_* biglake_*
        bigquery_* bigtable_* binary_* blockchain_* ces_* chronicle_* cloud_run_* cloud_scheduler_* cloud_tasks_* cloudbuild_*
        cloudbuildv2_* clouddeploy_* cloudfunctions_* cloudfunctions2_* colab_* composer_* config_* container_* data_* database_*
        dataflow_* dataform_* dataplex_* dataproc_* datastream_* deployment_* developer_* dialogflow_* discovery_* document_*
        edgecontainer_* edgenetwork_* endpoints_* eventarc_* filestore_* firebase_* firebaserules_* firestore_* gemini_* gke_* gkeonprem_*
        healthcare_* hypercomputecluster_* identity_* integration_* integrations_* kms_* license_* looker_* lustre_* managed_* memcache_*
        memorystore_* migration_* model_* netapp_* oracle_* parallelstore_* parameter_* privateca_* public_ca_* pubsub_* recaptcha_*
        redis_* scc_* secret_* secure_* securityposture_* sourcerepo_* spanner_* sql_* storage_* transcoder_* vector_* vertex_*
        vmwareengine_* workbench_* workflows_* workstations_*",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "google_compute_firewall google_compute_instance google_compute_network google_compute_subnetwork google_project
        google_project_service",
    ),
    ("provider", "region"),
    (
        "google_compute_firewall",
        "allow allow.ports allow.protocol direction name network project source_ranges target_tags",
    ),
    (
        "google_compute_instance",
        "boot_disk boot_disk.initialize_params boot_disk.initialize_params.image boot_disk.initialize_params.size labels machine_type
        metadata metadata_startup_script name network_interface network_interface.access_config network_interface.network_ip
        network_interface.subnetwork project shielded_instance_config shielded_instance_config.enable_secure_boot
        shielded_instance_config.enable_vtpm tags zone",
    ),
    ("google_compute_network", "auto_create_subnetworks name project"),
    ("google_compute_subnetwork", "ip_cidr_range name network project region"),
    ("google_project", "auto_create_network billing_account deletion_policy name org_id project_id"),
    ("google_project_service", "disable_on_destroy project service"),
];

/// The provider block's settings.
const SETTINGS: Rules = &[
    (
        CREDENTIALS,
        "*_custom_endpoint access_token add_terraform_attribution_label billing_project credentials impersonate_service_account
        impersonate_service_account_delegates poll_interval prefer_global_endpoints prefer_regional_endpoints request_reason
        request_timeout scopes terraform_attribution_label_addition_strategy universe_domain user_project_override batching batching.*
        external_credentials external_credentials.*",
    ),
    (LOCATION, "project zone"),
    (TAGS, "default_labels"),
    (PROTECT, "deletion_policy"),
];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "google_compute_firewall",
        &[
            (PROTECT, "deletion_policy disabled"),
            (
                CLOUD,
                "enable_logging log_config log_config.* source_service_accounts target_service_accounts params params.*",
            ),
            (ALIAS, "source_tags priority deny deny.* destination_ranges"),
        ],
    ),
    (
        "google_compute_instance",
        &[
            (
                PROTECT,
                "allow_stopping_for_update deletion_policy deletion_protection desired_status key_revocation_action_type",
            ),
            (
                CLOUD,
                "enable_display min_cpu_platform resource_policies advanced_machine_features.enable_uefi_networking
                advanced_machine_features.performance_monitoring_unit advanced_machine_features.turbo_mode *encryption_key_raw *encryption_key_rsa
                *encryption_service_account *kms_key_self_link boot_disk.initialize_params.source_image_encryption_key
                boot_disk.initialize_params.source_image_encryption_key.* boot_disk.initialize_params.source_snapshot_encryption_key
                boot_disk.initialize_params.source_snapshot_encryption_key.* boot_disk.guest_os_features boot_disk.interface
                boot_disk.initialize_params.enable_confidential_compute boot_disk.initialize_params.provisioned_iops
                boot_disk.initialize_params.provisioned_throughput boot_disk.initialize_params.storage_pool
                boot_disk.initialize_params.resource_policies boot_disk.initialize_params.resource_manager_tags confidential_instance_config
                confidential_instance_config.* instance_encryption_key instance_encryption_key.* network_interface.igmp_query
                network_interface.nic_type network_interface.queue_count network_interface.network_attachment
                network_interface.access_config.network_tier network_interface.alias_ip_range network_interface.alias_ip_range.*
                network_performance_config network_performance_config.* params params.* scheduling.* service_account service_account.*
                shielded_instance_config.enable_integrity_monitoring workload_identity_config workload_identity_config.*",
            ),
            (ROUTER, "can_ip_forward"),
            (
                PLACEMENT,
                "reservation_affinity reservation_affinity.* boot_disk.initialize_params.replica_zones",
            ),
            (
                ALIAS,
                "hostname boot_disk.auto_delete boot_disk.device_name boot_disk.force_attach boot_disk.mode boot_disk.source
                boot_disk.initialize_params.snapshot boot_disk.initialize_params.type boot_disk.initialize_params.architecture
                network_interface.network network_interface.subnetwork_project network_interface.access_config.nat_ip",
            ),
            (CPUS, "advanced_machine_features.threads_per_core advanced_machine_features.visible_core_count"),
            (NESTED, "advanced_machine_features advanced_machine_features.enable_nested_virtualization"),
            (
                DISKS,
                "attached_disk attached_disk.device_name attached_disk.force_attach attached_disk.mode attached_disk.source scratch_disk
                scratch_disk.*",
            ),
            (ACCELERATORS, "guest_accelerator guest_accelerator.*"),
            (GPU_SCHEDULING, "scheduling scheduling.on_host_maintenance"),
            (
                GCP_IPV6,
                "network_interface.internal_ipv6_prefix_length network_interface.ipv6_address network_interface.stack_type
                network_interface.ipv6_access_config network_interface.ipv6_access_config.*",
            ),
            (PUBLIC_DNS, "network_interface.access_config.public_ptr_domain_name"),
            (VLANS, "network_interface.vlan"),
            (
                TPM,
                "shielded_instance_config shielded_instance_config.enable_secure_boot shielded_instance_config.enable_vtpm",
            ),
        ],
    ),
    (
        "google_compute_network",
        &[
            (PROTECT, "deletion_policy"),
            (
                CLOUD_NETWORK,
                "bgp_always_compare_med bgp_best_path_selection_mode bgp_inter_region_cost delete_bgp_always_compare_med routing_mode
                network_firewall_policy_enforcement_order network_profile params params.*",
            ),
            (ALIAS, "delete_default_routes_on_create"),
            (GCP_IPV6, "enable_ula_internal_ipv6 internal_ipv6_range"),
            (MTU, "mtu"),
        ],
    ),
    (
        "google_compute_subnetwork",
        &[
            (PROTECT, "deletion_policy"),
            (
                CLOUD_NETWORK,
                "allow_subnet_cidr_routes_overlap ip_collection private_ip_google_access private_ipv6_google_access purpose reserved_internal_range
                resolve_subnet_mask role send_secondary_ip_range_if_empty params params.*",
            ),
            (GCP_IPV6, "external_ipv6_prefix internal_ipv6_prefix ipv6_access_type stack_type"),
            (MONITORING, "log_config log_config.*"),
            (ADDRESSES, "secondary_ip_range secondary_ip_range.*"),
        ],
    ),
    ("google_project", &[(LOCATION, "folder_id")]),
    ("google_project_service", &[(PROTECT, "deletion_policy disable_dependent_services")]),
];
