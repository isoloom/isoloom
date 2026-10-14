//! Azure (hashicorp/azurerm): its resource types by family, and the arguments of what the
//! cloud-vm and cloud-docker targets write.

use super::support::{
    ACCOUNT, ALIAS, BACKUP, CLOUD, CLOUD_NETWORK, CLOUD_WRITTEN, CREDENTIALS, ENVIRONMENT, IMAGES, IPV6_GATED, MANAGED, OFFLINE, PLACEMENT, PRIVATE_DNS,
    PROTECT, PUBLIC_DNS, PUBLIC_IPV6, SOURCE_PORTS, TAGS, TPM, VM_AGENT,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Equivalent};

/// A Windows VM's TPM: written for one that has a TPM, which `cloud-vm` can't give a Windows
/// machine until AWS (whose refusal is the target's) has Amazon's NitroTPM Windows images.
const WINDOWS_TPM: Support = Support::Planned {
    note: "with a TPM on Windows in the cloud: AWS's NitroTPM Windows images (TPM-Windows_Server-*) first",
};

/// Rule resources whose effect the security groups' inline rules give.
const INLINE: Support = Equivalent {
    via: "the `security_rule` blocks of azurerm_network_security_group, which carry the reach rules",
};

/// Azure: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: Azure",
    file: ".isoloom/cloud-vm/azure/ and .isoloom/cloud-docker/azure/",
    source: "hashicorp/azurerm 5.8.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "azurerm",
        types: include_str!("../../coverage/terraform/azure.txt"),
        schema: include_str!("../../coverage/terraform/azure-schema.txt"),
    }],
    prefix: "azurerm_",
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
        "nat_gateway nat_gateway_public_ip_association subnet_nat_gateway_association route_table subnet_route_table_association
        managed_disk virtual_machine_data_disk_attachment ssh_public_key subnet_network_security_group_association
        virtual_network_dns_servers route",
    ),
    (
        INLINE,
        "network_security_rule application_security_group network_interface_application_security_group_association",
    ),
    (IMAGES, "image shared_image shared_image_gallery shared_image_version"),
    (
        PRIVATE_DNS,
        "private_dns_zone private_dns_zone_virtual_network_link private_dns_a_record private_dns_aaaa_record private_dns_cname_record
        private_dns_mx_record private_dns_ptr_record private_dns_srv_record private_dns_txt_record",
    ),
    (PUBLIC_DNS, "dns_* public_ip_prefix nat_gateway_public_ip_prefix_association"),
    (
        PLACEMENT,
        "availability_set proximity_placement_group dedicated_host dedicated_host_group capacity_reservation capacity_reservation_group
        linux_virtual_machine_scale_set windows_virtual_machine_scale_set orchestrated_virtual_machine_scale_set virtual_machine_scale_set
        virtual_machine_scale_set_* virtual_machine",
    ),
    (
        BACKUP,
        "snapshot virtual_machine_restore_point virtual_machine_restore_point_collection backup_* recovery_* site_recovery_*
        data_protection_* virtual_machine_implicit_data_disk_from_source",
    ),
    (
        VM_AGENT,
        "virtual_machine_extension virtual_machine_run_command virtual_machine_gallery_application_assignment gallery_application
        gallery_application_version",
    ),
    (
        CLOUD_NETWORK,
        "express_route_* firewall firewall_* lb lb_* local_network_gateway private_endpoint private_endpoint_* private_link_service
        private_dns_resolver* route_filter route_map route_server route_server_* virtual_hub* virtual_network_gateway*
        virtual_network_peering virtual_wan vpn_* bastion_host network_ddos_protection_plan network_manager* network_security_perimeter*
        network_profile network_interface_application_gateway_backend_address_pool_association
        network_interface_backend_address_pool_association network_interface_nat_rule_association ip_group ip_group_cidr
        application_gateway* frontdoor_* cdn_* traffic_manager_* web_application_firewall_policy point_to_site_vpn_gateway
        subnet_service_endpoint_storage_policy resource_management_private_link* network_function_* palo_alto_* nginx_* custom_ip_prefix
        application_load_balancer*",
    ),
    (
        ACCOUNT,
        "role_* user_assigned_identity management_group* management_lock subscription subscription_* resource_group_* resource_policy_*
        resource_provider_* policy_* marketplace_* advisor_* consumption_* cost_* billing_* monitor_* log_analytics_*
        application_insights* network_watcher network_watcher_flow_log network_connection_monitor virtual_machine_packet_capture
        virtual_machine_automanage_configuration_assignment automanage_* tenant_* lighthouse_* portal_* dashboard_* security_center_*
        pim_* federated_identity_credential maintenance_* disk_access disk_encryption_set managed_disk_sas_token
        resource_deployment_script_* resource_group_template_deployment",
    ),
    (
        MANAGED,
        "aadb2c_* active_directory_* advanced_threat_protection ai_* analysis_* api_* app_* arc_* attestation_* automation_* batch_*
        blueprint_* bot_* chaos_* cognitive_* communication_* confidential_* container_* cosmosdb_* custom_provider data_* database_*
        databox_* databricks_* datadog_* dedicated_hardware_security_module dev_* digital_* dynatrace_* elastic_* email_* eventgrid_*
        eventhub eventhub_* extended_* fabric_* fluid_* function_* graph_* hdinsight_* healthbot healthcare_* iot_* iotcentral_* iothub
        iothub_* key_vault* kubernetes_* kusto_* linux_function_app* linux_web_app* load_test logic_* machine_* managed_application*
        managed_devops_pool managed_lustre_file_system managed_redis* maps_* mongo_* mssql_* mysql_* netapp_* new_relic_* notification_*
        oracle_* playwright_* postgresql_* powerbi_* purview_* qumulo_* redhat_* redis_* relay_* search_* sentinel_* service_*
        servicebus_* signalr_* source_control_token spring_* stack_* static_* storage_* stream_* synapse_* system_* trusted_* video_*
        virtual_desktop_* vmware_* web_* windows_function_app* windows_web_app* workloads_*",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "azurerm_linux_virtual_machine azurerm_network_interface azurerm_network_interface_security_group_association
        azurerm_network_security_group azurerm_public_ip azurerm_resource_group azurerm_subnet azurerm_virtual_network
        azurerm_windows_virtual_machine",
    ),
    ("provider", "features subscription_id"),
    (
        "azurerm_linux_virtual_machine",
        "admin_ssh_key admin_ssh_key.public_key admin_ssh_key.username admin_username custom_data location name network_interface_ids
        os_disk os_disk.caching os_disk.disk_size_gb os_disk.storage_account_type resource_group_name size source_image_reference
        secure_boot_enabled source_image_reference.offer source_image_reference.publisher source_image_reference.sku
        source_image_reference.version tags vtpm_enabled",
    ),
    (
        "azurerm_network_interface",
        "ip_configuration ip_configuration.name ip_configuration.private_ip_address ip_configuration.private_ip_address_allocation
        ip_configuration.public_ip_address_id ip_configuration.subnet_id ip_forwarding_enabled location name resource_group_name tags",
    ),
    (
        "azurerm_network_interface_security_group_association",
        "network_interface_id network_security_group_id",
    ),
    (
        "azurerm_network_security_group",
        "location name resource_group_name security_rule security_rule.access security_rule.destination_address_prefix
        security_rule.destination_port_range security_rule.destination_port_ranges security_rule.direction security_rule.name
        security_rule.priority security_rule.protocol security_rule.source_address_prefix security_rule.source_port_range tags",
    ),
    ("azurerm_public_ip", "allocation_method location name resource_group_name sku tags"),
    ("azurerm_resource_group", "location name tags"),
    ("azurerm_subnet", "address_prefixes name resource_group_name virtual_network_name"),
    ("azurerm_virtual_network", "address_space location name resource_group_name tags"),
    (
        "azurerm_windows_virtual_machine",
        "admin_password admin_username computer_name location name network_interface_ids os_disk os_disk.caching os_disk.disk_size_gb
        os_disk.storage_account_type resource_group_name size source_image_reference source_image_reference.offer
        source_image_reference.publisher source_image_reference.sku source_image_reference.version tags winrm_listener
        winrm_listener.protocol",
    ),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "ado_pipeline_service_connection_id auxiliary_tenant_ids client_certificate client_certificate_password client_certificate_path
        client_id client_id_file_path client_secret client_secret_file_path disable_correlation_request_id disable_terraform_partner_id
        environment metadata_host msi_api_version msi_endpoint oidc_request_token oidc_request_url oidc_token oidc_token_file_path
        partner_id resource_provider_registrations resource_providers_to_register storage_use_azuread tenant_id use_aks_workload_identity
        use_cli use_msi use_oidc features.*",
)];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "azurerm_linux_virtual_machine",
        &[
            (
                PLACEMENT,
                "availability_set_id capacity_reservation_group_id dedicated_host_group_id dedicated_host_id edge_zone eviction_policy
                max_bid_price platform_fault_domain priority proximity_placement_group_id virtual_machine_scale_set_id zone license_type plan
                plan.*",
            ),
            (PROTECT, "bypass_platform_safety_checks_on_user_schedule_enabled extensions_time_budget"),
            (
                CLOUD,
                "allow_extension_operations provision_vm_agent patch_assessment_mode patch_mode reboot_setting disk_controller_type
                encryption_at_host_enabled additional_capabilities additional_capabilities.* boot_diagnostics boot_diagnostics.*
                gallery_application gallery_application.* identity identity.* os_disk.disk_encryption_set_id
                os_disk.secure_vm_disk_encryption_set_id os_disk.security_encryption_type os_disk.write_accelerator_enabled
                os_disk.diff_disk_settings os_disk.diff_disk_settings.* os_image_notification os_image_notification.* secret secret.*
                termination_notification termination_notification.*",
            ),
            (ALIAS, "os_disk.name user_data os_managed_disk_id"),
            (IMAGES, "source_image_id"),
            (TPM, "secure_boot_enabled vtpm_enabled"),
            (ALIAS, "admin_password disable_password_authentication computer_name"),
        ],
    ),
    (
        "azurerm_windows_virtual_machine",
        &[
            (
                PLACEMENT,
                "availability_set_id capacity_reservation_group_id dedicated_host_group_id dedicated_host_id edge_zone eviction_policy
                max_bid_price platform_fault_domain priority proximity_placement_group_id virtual_machine_scale_set_id zone license_type plan
                plan.*",
            ),
            (PROTECT, "bypass_platform_safety_checks_on_user_schedule_enabled extensions_time_budget"),
            (
                CLOUD,
                "allow_extension_operations provision_vm_agent patch_assessment_mode patch_mode reboot_setting disk_controller_type
                encryption_at_host_enabled additional_capabilities additional_capabilities.* boot_diagnostics boot_diagnostics.*
                gallery_application gallery_application.* identity identity.* os_disk.disk_encryption_set_id
                os_disk.secure_vm_disk_encryption_set_id os_disk.security_encryption_type os_disk.write_accelerator_enabled
                os_disk.diff_disk_settings os_disk.diff_disk_settings.* os_image_notification os_image_notification.* secret secret.*
                termination_notification termination_notification.*",
            ),
            (ALIAS, "os_disk.name user_data os_managed_disk_id"),
            (IMAGES, "source_image_id"),
            (WINDOWS_TPM, "secure_boot_enabled vtpm_enabled"),
            (
                CLOUD,
                "automatic_updates_enabled hotpatching_enabled timezone additional_unattend_content additional_unattend_content.*
                winrm_listener.certificate_url",
            ),
            (ALIAS, "custom_data"),
        ],
    ),
    (
        "azurerm_network_security_group",
        &[
            (
                ALIAS,
                "security_rule.destination_address_prefixes security_rule.destination_application_security_group_ids
                security_rule.source_address_prefixes security_rule.source_application_security_group_ids",
            ),
            (SOURCE_PORTS, "security_rule.source_port_ranges"),
        ],
    ),
    (
        "azurerm_network_interface",
        &[
            (
                CLOUD,
                "accelerated_networking_enabled auxiliary_mode auxiliary_sku edge_zone
                ip_configuration.gateway_load_balancer_frontend_ip_configuration_id",
            ),
            (PRIVATE_DNS, "dns_servers internal_dns_name_label"),
            (ALIAS, "ip_configuration.primary"),
            (IPV6_GATED, "ip_configuration.private_ip_address_version"),
        ],
    ),
    (
        "azurerm_public_ip",
        &[
            (
                CLOUD,
                "ddos_protection_mode ddos_protection_plan_id edge_zone idle_timeout_in_minutes ip_tags public_ip_prefix_id sku_tier zones",
            ),
            (PUBLIC_DNS, "domain_name_label domain_name_label_scope reverse_fqdn"),
            (PUBLIC_IPV6, "ip_version"),
        ],
    ),
    ("azurerm_resource_group", &[(TAGS, "managed_by")]),
    (
        "azurerm_subnet",
        &[
            (
                CLOUD,
                "private_endpoint_network_policies private_link_service_network_policies_enabled service_endpoint_policy_ids sharing_scope
                delegation delegation.* service_endpoint service_endpoint.* ip_address_pool ip_address_pool.*",
            ),
            (
                ALIAS,
                "network_security_group_id_wo network_security_group_id_wo_version route_table_id_wo route_table_id_wo_version",
            ),
            (OFFLINE, "default_outbound_access_enabled"),
        ],
    ),
    (
        "azurerm_virtual_network",
        &[
            (
                CLOUD,
                "bgp_community edge_zone flow_timeout_in_minutes private_endpoint_vnet_policies ddos_protection_plan ddos_protection_plan.*
                encryption encryption.* ip_address_pool ip_address_pool.*",
            ),
            (PRIVATE_DNS, "dns_servers"),
            (ALIAS, "subnet subnet.*"),
        ],
    ),
];
