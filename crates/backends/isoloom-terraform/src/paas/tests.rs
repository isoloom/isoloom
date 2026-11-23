use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::CloudVmBackend;
use crate::cloud::Cloud;
use crate::testing::environment;
use isoloom_test_support::{hcl_attribute, hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_resource, hcl_text, hcl_values};

/// A shop: its web machine (a bucket and Kafka client with its keys), its worker (the same with
/// its identity), its database, its bucket and its brokers.
const SPEC: &str = "version: 1\nname: shop\nnetworks: { lab: { cidr: 10.30.0.0/24 } }\nsecrets:\n  db-password: { env: SHOP_DB_PASSWORD }\n  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\n  bus-user: { env: SHOP_BUS_USER }\n  bus-password: { env: SHOP_BUS_PASSWORD }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] } }\n  worker: { os: debian@12, networks: { lab: 11 }, app: { steps: [{ packages: [curl] }] } }\nservices:\n  db:\n    kind: postgres\n    version: \"16\"\n    networks: { lab: 40 }\n    database: shop\n    user: shop\n    password: db-password\n    clients: { web: { env_prefix: DATABASE } }\n  files:\n    kind: object-storage\n    bucket: uploads\n    api: platform\n    versioning: true\n    networks: { lab: 41 }\n    credentials: { access_key: files-key, secret_key: files-secret }\n    clients: { web: { env_prefix: STORAGE }, worker: { env_prefix: STORAGE, access: identity } }\n  bus:\n    kind: kafka\n    networks: { lab: 42 }\n    topics: { orders: { partitions: 3 } }\n    credentials: { user: bus-user, password: bus-password }\n    clients: { web: {}, worker: { access: identity } }\n";

fn files(yaml: &str) -> Vec<isoloom_render::GeneratedFile> {
    CloudVmBackend.render(&environment(yaml, Target::CloudVm)).unwrap()
}

fn file(yaml: &str, cloud: Cloud, name: &str) -> String {
    let path = format!(".isoloom/cloud-vm/{}/{name}", cloud.id());
    files(yaml).into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}")).contents
}

/// The files a reviewer reads as files: each cloud's services.tf.
#[test]
fn each_cloud_renders_its_managed_services_beside_the_module() {
    for (cloud, name) in [
        (Cloud::Aws, "aws_renders_rds_s3_and_msk_beside_the_module"),
        (Cloud::Azure, "azure_renders_a_flexible_server_a_storage_container_and_event_hubs"),
        (Cloud::Gcp, "google_cloud_renders_cloud_sql_a_bucket_and_managed_kafka"),
    ] {
        insta::assert_snapshot!(name, file(SPEC, cloud, "services.tf"));
    }
}

/// The comment lines of `tf`, as one text.
fn comments(tf: &str) -> String {
    tf.lines().filter_map(|l| l.strip_prefix("# ")).collect::<Vec<_>>().join(" ")
}

/// The one `resource "<kind>" "<name>"` of `tf`, as HCL writes it: what a test reads within it.
fn resource(tf: &str, kind: &str, name: &str) -> String {
    hcl::format::to_string(&hcl_resource(tf, kind, name)).unwrap_or_default()
}

/// The `variable` blocks `tf` declares.
fn variables(tf: &str) -> Vec<String> {
    hcl_blocks(&hcl_body(tf), "variable", &[])
        .iter()
        .filter_map(|v| v.labels().first().map(|l| l.as_str().to_owned()))
        .collect()
}

#[test]
fn services_tf_says_the_zone_is_on_and_billed() {
    let header = comments(&file(SPEC, Cloud::Aws, "services.tf"));
    assert!(header.contains("`dns.private` is on: a spec with services turns it on"), "{header}");
    assert!(header.contains("Route 53 private hosted zone, about $0.50 a month"), "{header}");
}

#[test]
fn the_facts_a_cloud_issues_are_delivered_after_the_clients_set_up() {
    let services = file(SPEC, Cloud::Aws, "services.tf");
    let to_web = hcl_resource(&services, "terraform_data", "isoloom_service_files_to_web");
    let after: String = hcl_text(&to_web, "depends_on").split_whitespace().collect();
    assert_eq!(after, "[terraform_data.isoloom_ready_web]");
    // Its key's secret, issued by the cloud, uploaded and installed where the client reads it.
    let uploads: Vec<(String, String)> = hcl_nested(&to_web, "provisioner")
        .iter()
        .filter(|p| p.labels().first().is_some_and(|l| l.as_str() == "file"))
        .map(|p| (hcl_text(p.body(), "content"), hcl_text(p.body(), "destination")))
        .collect();
    assert!(
        uploads.contains(&(
            "aws_iam_access_key.isoloom_service_files.secret".to_owned(),
            "/tmp/isoloom-service-files-secret-key".to_owned()
        )),
        "{uploads:?}"
    );
    assert!(hcl_literals(&to_web).iter().any(|l| l.contains("/etc/isoloom/services/files/secret-key")));
    // The identity client gets no key, its identity instead.
    let worker = resource(&services, "terraform_data", "isoloom_service_files_to_worker");
    assert!(
        !worker.contains("secret-key") && worker.contains("aws_iam_role.isoloom_client_worker.arn"),
        "{worker}"
    );
}

#[test]
fn an_identity_client_runs_as_its_identity_in_main_tf() {
    let aws = file(SPEC, Cloud::Aws, "main.tf");
    let worker = hcl_resource(&aws, "aws_instance", "worker");
    assert_eq!(hcl_text(&worker, "iam_instance_profile"), "aws_iam_instance_profile.isoloom_client_worker.name");
    let web = hcl_resource(&aws, "aws_instance", "web");
    assert!(hcl_attribute(&web, "iam_instance_profile").is_none(), "only the identity client");
    let azure = hcl_resource(&file(SPEC, Cloud::Azure, "main.tf"), "azurerm_linux_virtual_machine", "worker");
    assert_eq!(hcl_values(&azure, "type"), ["UserAssigned"]);
    let gcp = resource(&file(SPEC, Cloud::Gcp, "main.tf"), "google_compute_instance", "worker");
    assert!(gcp.contains("google_service_account.isoloom_client_worker.email"), "{gcp}");
}

#[test]
fn the_password_is_the_secret_variable_the_client_reads() {
    // Declared once, in main.tf, whose client reads it; MSK's login, which no machine reads, in
    // services.tf.
    assert!(variables(&file(SPEC, Cloud::Aws, "main.tf")).contains(&"ISOLOOM_SECRET_DB_PASSWORD".to_owned()));
    let services = file(SPEC, Cloud::Aws, "services.tf");
    let declared = variables(&services);
    assert!(!declared.contains(&"ISOLOOM_SECRET_DB_PASSWORD".to_owned()), "{declared:?}");
    assert!(declared.contains(&"ISOLOOM_SECRET_BUS_PASSWORD".to_owned()), "{declared:?}");
    assert_eq!(
        hcl_text(&hcl_resource(&services, "aws_db_instance", "db"), "password"),
        "var.ISOLOOM_SECRET_DB_PASSWORD"
    );
}

#[test]
fn a_published_database_is_allow_listed_by_its_input() {
    let published = SPEC.replace("secrets:\n", "inputs:\n  PLAYER_IP: { sensitive: false }\nsecrets:\n").replace(
        "    clients: { web: { env_prefix: DATABASE } }\n",
        "    clients: { web: { env_prefix: DATABASE } }\n    publish: { allow: PLAYER_IP }\n",
    );
    let aws = file(&published, Cloud::Aws, "services.tf");
    assert_eq!(hcl_text(&hcl_resource(&aws, "aws_db_instance", "db"), "publicly_accessible"), "true");
    let group = resource(&aws, "aws_security_group", "isoloom_service_db");
    assert!(group.contains("var.PLAYER_IP"), "{group}");
    assert!(variables(&aws).contains(&"PLAYER_IP".to_owned()));
    let azure = hcl_body(&file(&published, Cloud::Azure, "services.tf"));
    assert_ne!(
        hcl_names(&azure, "resource", "azurerm_postgresql_flexible_server_firewall_rule"),
        Vec::<String>::new()
    );
    let gcp = hcl_body(&file(&published, Cloud::Gcp, "services.tf"));
    let sql: Vec<&hcl::Block> = hcl_blocks(&gcp, "resource", &["google_sql_database_instance"]);
    let networks = sql
        .iter()
        .flat_map(|i| hcl_nested(i.body(), "settings"))
        .flat_map(|s| hcl_nested(s.body(), "ip_configuration"))
        .flat_map(|c| hcl_nested(c.body(), "authorized_networks"))
        .count();
    assert!(networks > 0, "Cloud SQL's authorized networks");
}

#[test]
fn the_clouds_without_services_refuse_and_azure_refuses_s3() {
    let env = environment(SPEC, Target::CloudVm);
    let refusals = CloudVmBackend.refusals(&env);
    let refused = |c: Cloud| refusals.iter().find(|(x, _)| *x == c).and_then(|(_, why)| why.clone());
    assert!(refused(Cloud::Linode).is_some_and(|w| w.contains("managed services")), "{refusals:?}");
    assert_eq!(refused(Cloud::Aws), None);
    let s3 = environment(&SPEC.replace("    api: platform\n", ""), Target::CloudVm);
    assert!(crate::paas::refusal(Cloud::Azure, &s3).is_some_and(|w| w.contains("no S3 API")));
}

#[test]
fn the_kafka_limits_refuse_the_one_cloud_they_bind() {
    let refused = |topics: &str| {
        let env = environment(&SPEC.replace("topics: { orders: { partitions: 3 } }", topics), Target::CloudVm);
        CloudVmBackend
            .refusals(&env)
            .into_iter()
            .filter_map(|(c, why)| why.filter(|w| w.contains("services.bus.topics")).map(|w| (c, w)))
            .collect::<Vec<_>>()
    };
    let compact = refused("topics: { ledger: { cleanup: compact } }");
    assert_eq!(compact.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Cloud::Aws, Cloud::Azure], "{compact:?}");
    assert!(
        compact[1].1.contains("Event Hubs Standard compacts no topic") && compact[1].1.contains("`cleanup: delete`"),
        "{compact:?}"
    );
    let wide = refused("topics: { orders: { partitions: 33 } }");
    assert_eq!(wide.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Cloud::Azure], "{wide:?}");
    let mixed = refused("topics: { orders: { partitions: 3 }, audit: { partitions: 1 } }");
    assert_eq!(mixed.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Cloud::Aws], "{mixed:?}");
    assert!(mixed[0].1.contains("one partition count"), "{mixed:?}");
}

#[test]
fn a_service_is_reached_from_what_the_spec_defines_and_nothing_else() {
    let two = SPEC
        .replace(
            "networks: { lab: { cidr: 10.30.0.0/24 } }",
            "networks: { lab: { cidr: 10.30.0.0/24 }, ops: { cidr: 10.32.0.0/24 }, dmz: { cidr: 10.33.0.0/24 } }\nreach: [{ from: ops, to: lab }]",
        )
        .replace(
            "worker: { os: debian@12, networks: { lab: 11 }",
            "worker: { os: debian@12, networks: { lab: 11, dmz: 11 }",
        );
    let services = file(&two, Cloud::Aws, "services.tf");
    // Its network and the one reach lets in; a client's other network gives no access.
    let group = hcl_resource(&services, "aws_security_group", "isoloom_service_db");
    assert_eq!(hcl_values(&group, "cidr_blocks"), ["10.30.0.0/24", "10.32.0.0/24"]);
}
