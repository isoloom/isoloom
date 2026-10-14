//! AWS (hashicorp/aws): its resource types by family, and the arguments of what the
//! cloud-vm and cloud-docker targets write.

use super::support::{
    ACCOUNT, ADDRESSES, ALIAS, AWS_IPV6, BACKUP, CLOUD, CLOUD_NAMES, CLOUD_NETWORK, CLOUD_RESOLVER, CLOUD_WRITTEN, CPUS, CREDENTIALS, DEFAULTS, DISKS,
    IMAGE_BUILD, IMAGES, LOCATION, MANAGED, NAT, NESTED, PLACEMENT, PROTECT, PUBLIC_DNS, ROUTER, ZONE_BINDING, ZONE_SETTINGS,
};
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Equivalent};

/// Rule resources whose effect the security groups' inline rules give.
const INLINE: Support = Equivalent {
    via: "the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules",
};

/// Resources that wire up what Isoloom writes inline.
const WIRED: Support = Equivalent {
    via: "the arguments that wire the same things inline: `vpc_id` of aws_internet_gateway, `network_interface` of aws_eip, an aws_route_table_association for every subnet (the main route table serves none), `security_groups` of aws_network_interface and the `route` blocks of aws_route_table",
};

/// AWS: every resource type, and what the generators write.
pub const TABLE: Table = Table {
    name: "Terraform: AWS",
    file: ".isoloom/cloud-vm/aws/ and .isoloom/cloud-docker/aws/",
    source: "hashicorp/aws 6.67.0 (its resource types, and the schema of what Isoloom writes)",
    providers: &[Provider {
        name: "aws",
        types: include_str!("../../coverage/terraform/aws.txt"),
        schema: include_str!("../../coverage/terraform/aws-schema.txt"),
    }],
    prefix: "aws_",
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
        WIRED,
        "internet_gateway_attachment eip_association main_route_table_association network_interface_sg_attachment route",
    ),
    (NAT, "nat_gateway nat_gateway_eip_association"),
    (AWS_IPV6, "vpc_ipv6_cidr_block_association egress_only_internet_gateway"),
    (DISKS, "ebs_volume volume_attachment"),
    (CLOUD_RESOLVER, "vpc_dhcp_options vpc_dhcp_options_association"),
    (
        INLINE,
        "security_group_rule vpc_security_group_ingress_rule vpc_security_group_egress_rule network_acl network_acl_association
        network_acl_rule",
    ),
    (IMAGES, "ami ami_copy ami_from_instance ebs_snapshot_import"),
    (ZONE_BINDING, "route53_zone_association route53_vpc_association_authorization"),
    (DEFAULTS, "default_*"),
    (
        PUBLIC_DNS,
        "route53_cidr_* route53_delegation_set route53_health_check route53_hosted_zone_dnssec route53_key_signing_key
        route53_traffic_policy* route53domains_* eip_domain_name",
    ),
    (
        PLACEMENT,
        "launch_template launch_configuration autoscaling_* autoscalingplans_* spot_* placement_group ec2_fleet ec2_host ec2_capacity_*
        ec2_availability_zone_group ec2_default_credit_specification ec2_instance_state ec2_subnet_cidr_reservation",
    ),
    (IMAGE_BUILD, "imagebuilder_*"),
    (
        BACKUP,
        "ebs_snapshot ebs_snapshot_copy ebs_volume_copy ebs_fast_snapshot_restore snapshot_create_volume_permission dlm_* backup_* rbin_*
        drs_*",
    ),
    (
        CLOUD_NETWORK,
        "ec2_transit_gateway* ec2_client_vpn_* ec2_carrier_gateway ec2_local_gateway_* ec2_traffic_mirror_* ec2_managed_prefix_list*
        ec2_instance_connect_endpoint ec2_secondary_* ec2_network_insights_* vpc_endpoint* vpc_peering_* vpc_route_server*
        vpc_security_group_vpc_association vpclattice_* vpn_* customer_gateway dx_* networkmanager_* lb lb_* alb alb_* elb elb_*
        load_balancer_* proxy_protocol_policy app_cookie_stickiness_policy globalaccelerator_* route53_resolver_* cloudfront_*
        cloudfrontkeyvaluestore_* verifiedaccess_* network_interface_permission",
    ),
    (
        ACCOUNT,
        "accessanalyzer_* account_* accountaccess_* ami_launch_permission auditmanager_* billing_* budgets_* ce_* chatbot_*
        cloudcontrolapi_* cloudformation_* cloudtrail_* cloudwatch_* computeoptimizer_* config_* controltower_* costoptimizationhub_*
        cur_* ebs_default_kms_key ebs_encryption_by_default ebs_snapshot_block_public_access ec2_allowed_images_settings
        ec2_image_block_public_access ec2_instance_metadata_defaults ec2_serial_console_access ec2_tag flow_log iam_* identitystore_*
        internetmonitor_* invoicing_* licensemanager_* networkflowmonitor_* networkmonitor_* notifications_* notificationscontacts_* oam_*
        observabilityadmin_* organizations_* ram_* resourceexplorer2_* resourcegroups_* rolesanywhere_* route53_query_log
        route53_records_exclusive savingsplans_* servicecatalog_* servicecatalogappregistry_* servicequotas_* ssoadmin_*
        vpc_block_public_access_* vpc_encryption_control vpc_ipam* vpc_network_performance_metric_subscription
        vpc_security_group_rules_exclusive fms_* resiliencehub_* resiliencehubv2_* arcregionswitch_* arczonalshift_* uxc_*",
    ),
    (
        MANAGED,
        "acm_* acmpca_* agentregistry_* amplify_* api_* apigatewayv2_* app_* appautoscaling_* appconfig_* appfabric_* appflow_*
        appintegrations_* applicationinsights_* appmesh_* apprunner_* appstream_* appsync_* athena_* batch_* bcmdataexports_* bedrock_*
        bedrockagent_* bedrockagentcore_* chime_* chimesdkmediapipelines_* chimesdkvoice_* cleanrooms_* cloud9_* cloudhsm_* cloudsearch_*
        codeartifact_* codebuild_* codecatalyst_* codecommit_* codeconnections_* codedeploy_* codeguruprofiler_* codegurureviewer_*
        codepipeline_* codestarconnections_* codestarnotifications_* cognito_* comprehend_* connect_* customerprofiles_* dataexchange_*
        datapipeline_* datasync_* datazone_* dax_* db_* detective_* devicefarm_* devopsguru_* directory_* directoryservicedata_* dms_*
        docdb_* docdbelastic_* dsql_* dynamodb_* ecr_* ecrpublic_* ecs_* efs_* eks_* elastic_* elasticache_* elasticsearch_*
        elastictranscoder_* emr_* emrcontainers_* emrserverless_* evidently_* finspace_* fis_* fsx_* gamelift_* glacier_* glue_* grafana_*
        guardduty_* inspector_* inspector2_* iot_* ivs_* ivschat_* kendra_* keyspaces_* kinesis_* kinesisanalyticsv2_* kms_*
        lakeformation_* lambda_* lambdacore_* lambdamicrovms_* lex_* lexv2models_* lightsail_* location_* m2_* macie2_* mailmanager_*
        media_* medialive_* memorydb_* mq_* msk_* mskconnect_* mwaa_* neptune_* neptunegraph_* networkfirewall_* odb_* opensearch_*
        opensearchserverless_* osis_* outposts_* paymentcryptography_* pinpoint_* pinpointsmsvoicev2_* pipes_* prometheus_* qbusiness_*
        qldb_* quicksight_* rds_* redshift_* redshiftdata_* redshiftserverless_* rekognition_* route53profiles_*
        route53recoverycontrolconfig_* route53recoveryreadiness_* rum_* s3_* s3control_* s3files_* s3outposts_* s3tables_* s3vectors_*
        sagemaker_* scheduler_* schemas_* secretsmanager_* securityhub_* securitylake_* serverlessapplicationrepository_* service_* ses_*
        sesv2_* sfn_* shield_* signer_* sns_* sqs_* ssm_* ssmcontacts_* ssmincidents_* ssmquicksetup_* storagegateway_* swf_* synthetics_*
        timestreaminfluxdb_* timestreamquery_* timestreamwrite_* transcribe_* transfer_* verifiedpermissions_* waf_* wafregional_* wafv2_*
        workmail_* workspaces_* workspacesweb_* xray_*",
    ),
];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    (
        "types",
        "data.aws_ami data.aws_availability_zones aws_eip aws_instance aws_internet_gateway aws_key_pair aws_network_interface
        aws_network_interface_attachment aws_route53_record aws_route53_zone aws_route_table aws_route_table_association
        aws_security_group aws_subnet aws_vpc aws_vpc_ipv4_cidr_block_association",
    ),
    ("provider", "default_tags default_tags.tags region"),
    ("aws_eip", "network_interface"),
    (
        "aws_instance",
        "ami associate_public_ip_address instance_initiated_shutdown_behavior instance_type key_name metadata_options
        metadata_options.http_tokens primary_network_interface primary_network_interface.network_interface_id private_ip root_block_device
        root_block_device.encrypted root_block_device.volume_size root_block_device.volume_type subnet_id tags user_data
        vpc_security_group_ids",
    ),
    ("aws_internet_gateway", "tags vpc_id"),
    ("aws_key_pair", "key_name public_key"),
    ("aws_network_interface", "private_ips security_groups source_dest_check subnet_id tags"),
    ("aws_network_interface_attachment", "device_index instance_id network_interface_id"),
    ("aws_route53_record", "name records ttl type zone_id"),
    ("aws_route53_zone", "name vpc vpc.vpc_id"),
    ("aws_route_table", "route route.cidr_block route.gateway_id tags vpc_id"),
    ("aws_route_table_association", "route_table_id subnet_id"),
    (
        "aws_security_group",
        "egress egress.cidr_blocks egress.from_port egress.protocol egress.to_port ingress ingress.cidr_blocks ingress.description
        ingress.from_port ingress.protocol ingress.to_port name vpc_id",
    ),
    ("aws_subnet", "availability_zone cidr_block map_public_ip_on_launch tags vpc_id"),
    ("aws_vpc", "cidr_block enable_dns_hostnames enable_dns_support tags"),
    ("aws_vpc_ipv4_cidr_block_association", "cidr_block vpc_id"),
    ("data.aws_ami", "filter filter.name filter.values most_recent owners"),
    ("data.aws_availability_zones", "state"),
];

/// The provider block's settings.
const SETTINGS: Rules = &[(
    CREDENTIALS,
    "access_key secret_key token profile shared_config_files shared_credentials_files assume_role assume_role.*
        assume_role_with_web_identity assume_role_with_web_identity.* allowed_account_ids forbidden_account_ids sts_region
        custom_ca_bundle ec2_metadata_service_endpoint ec2_metadata_service_endpoint_mode http_proxy https_proxy no_proxy insecure
        max_retries retry_mode endpoints endpoints.* skip_credentials_validation skip_metadata_api_check skip_region_validation
        skip_requesting_account_id s3_us_east_1_regional_endpoint s3_use_path_style token_bucket_rate_limiter_capacity
        use_dualstack_endpoint use_fips_endpoint user_agent ignore_tags ignore_tags.* tag_policy_compliance",
)];

/// The arguments Isoloom leaves out, per resource type or data source.
const ARGUMENTS: &[(&str, Rules)] = &[
    (
        "aws_route53_record",
        &[
            (
                ZONE_SETTINGS,
                "allow_overwrite health_check_id multivalue_answer_routing_policy set_identifier cidr_routing_policy.*
                failover_routing_policy.* geolocation_routing_policy.* geoproximity_routing_policy.* latency_routing_policy.*
                weighted_routing_policy.*",
            ),
            (CLOUD_NETWORK, "alias.*"),
        ],
    ),
    (
        "aws_route53_zone",
        &[
            (ZONE_SETTINGS, "delegation_set_id enable_accelerated_recovery"),
            (PROTECT, "force_destroy"),
            (ALIAS, "comment"),
            (LOCATION, "vpc.vpc_region"),
        ],
    ),
    (
        "aws_instance",
        &[
            (LOCATION, "availability_zone region"),
            (PROTECT, "disable_api_stop disable_api_termination force_destroy"),
            (
                CLOUD,
                "ebs_optimized enclave_options enclave_options.* hibernation get_password_data iam_instance_profile monitoring maintenance_options
                maintenance_options.* credit_specification credit_specification.* metadata_options.http_endpoint
                metadata_options.http_protocol_ipv6 metadata_options.http_put_response_hop_limit metadata_options.instance_metadata_tags
                private_dns_name_options private_dns_name_options.* cpu_options.amd_sev_snp root_block_device.iops root_block_device.throughput
                root_block_device.kms_key_id ebs_block_device.iops ebs_block_device.throughput ebs_block_device.kms_key_id
                ebs_block_device.snapshot_id ebs_block_device.encrypted ephemeral_block_device ephemeral_block_device.*",
            ),
            (ROUTER, "source_dest_check"),
            (
                PLACEMENT,
                "host_id host_resource_group_arn placement_group placement_group_id placement_partition_number tenancy
                capacity_reservation_specification capacity_reservation_specification.* instance_market_options instance_market_options.*
                launch_template launch_template.*",
            ),
            (AWS_IPV6, "enable_primary_ipv6 ipv6_address_count ipv6_addresses"),
            (ADDRESSES, "secondary_private_ips"),
            (
                ALIAS,
                "security_groups user_data_base64 user_data_replace_on_change volume_tags root_block_device.delete_on_termination network_interface
                network_interface.* secondary_network_interface secondary_network_interface.*",
            ),
            (CPUS, "cpu_options.core_count cpu_options.threads_per_core"),
            (NESTED, "cpu_options cpu_options.nested_virtualization"),
            (
                DISKS,
                "ebs_block_device ebs_block_device.delete_on_termination ebs_block_device.device_name ebs_block_device.volume_size
                ebs_block_device.volume_type",
            ),
        ],
    ),
    (
        "aws_eip",
        &[
            (LOCATION, "region network_border_group"),
            (CLOUD, "customer_owned_ipv4_pool ipam_pool_id public_ipv4_pool address"),
            (ALIAS, "instance associate_with_private_ip domain"),
        ],
    ),
    ("aws_internet_gateway", &[(LOCATION, "region")]),
    ("aws_key_pair", &[(LOCATION, "region"), (ALIAS, "key_name_prefix")]),
    (
        "aws_network_interface",
        &[
            (LOCATION, "region"),
            (
                AWS_IPV6,
                "enable_primary_ipv6 ipv6_address_count ipv6_address_list ipv6_address_list_enabled ipv6_addresses ipv6_prefix_count ipv6_prefixes",
            ),
            (
                ADDRESSES,
                "private_ips_count private_ip_list private_ip_list_enabled ipv4_prefix_count ipv4_prefixes",
            ),
            (ALIAS, "private_ip attachment attachment.*"),
            (CLOUD, "interface_type ena_srd_specification ena_srd_specification.*"),
        ],
    ),
    ("aws_network_interface_attachment", &[(LOCATION, "region"), (CLOUD, "network_card_index")]),
    (
        "aws_route_table",
        &[
            (LOCATION, "region"),
            (
                CLOUD_NETWORK,
                "propagating_vgws route.carrier_gateway_id route.core_network_arn route.destination_prefix_list_id route.local_gateway_id
                route.odb_network_arn route.transit_gateway_id route.vpc_endpoint_id route.vpc_peering_connection_id",
            ),
            (AWS_IPV6, "route.ipv6_cidr_block route.egress_only_gateway_id"),
            (NAT, "route.nat_gateway_id"),
            (ROUTER, "route.network_interface_id"),
        ],
    ),
    ("aws_route_table_association", &[(LOCATION, "region"), (CLOUD_NETWORK, "gateway_id")]),
    (
        "aws_security_group",
        &[
            (LOCATION, "region"),
            (
                ALIAS,
                "name_prefix revoke_rules_on_delete egress.prefix_list_ids egress.security_groups egress.self ingress.prefix_list_ids
                ingress.security_groups ingress.self",
            ),
            (AWS_IPV6, "egress.ipv6_cidr_blocks ingress.ipv6_cidr_blocks"),
        ],
    ),
    (
        "aws_subnet",
        &[
            (LOCATION, "region availability_zone_id outpost_arn"),
            (
                AWS_IPV6,
                "assign_ipv6_address_on_creation enable_dns64 enable_resource_name_dns_aaaa_record_on_launch ipv6_cidr_block ipv6_native
                ipv6_netmask_length ipv6_ipam_pool_id",
            ),
            (
                CLOUD,
                "customer_owned_ipv4_pool map_customer_owned_ip_on_launch enable_lni_at_device_index ipv4_ipam_pool_id ipv4_netmask_length",
            ),
            (CLOUD_NAMES, "enable_resource_name_dns_a_record_on_launch private_dns_hostname_type_on_launch"),
        ],
    ),
    (
        "aws_vpc",
        &[
            (LOCATION, "region"),
            (
                AWS_IPV6,
                "assign_generated_ipv6_cidr_block ipv6_cidr_block ipv6_cidr_block_network_border_group ipv6_ipam_pool_id ipv6_netmask_length",
            ),
            (CLOUD, "enable_network_address_usage_metrics ipv4_ipam_pool_id ipv4_netmask_length"),
            (PLACEMENT, "instance_tenancy"),
        ],
    ),
    (
        "aws_vpc_ipv4_cidr_block_association",
        &[(LOCATION, "region"), (CLOUD, "ipv4_ipam_pool_id ipv4_netmask_length")],
    ),
    (
        "data.aws_ami",
        &[
            (LOCATION, "region"),
            (ALIAS, "name_regex executable_users include_deprecated allow_unsafe_filter tags uefi_data"),
        ],
    ),
    (
        "data.aws_availability_zones",
        &[(LOCATION, "region all_availability_zones exclude_names exclude_zone_ids filter filter.*")],
    ),
];
