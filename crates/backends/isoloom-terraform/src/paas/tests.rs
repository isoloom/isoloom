use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::CloudVmBackend;
use crate::cloud::Cloud;
use crate::testing::environment;

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

#[test]
fn aws_renders_rds_s3_and_msk_beside_the_module() {
    insta::assert_snapshot!(file(SPEC, Cloud::Aws, "services.tf"));
}

#[test]
fn azure_renders_a_flexible_server_a_storage_container_and_event_hubs() {
    insta::assert_snapshot!(file(SPEC, Cloud::Azure, "services.tf"));
}

#[test]
fn google_cloud_renders_cloud_sql_a_bucket_and_managed_kafka() {
    insta::assert_snapshot!(file(SPEC, Cloud::Gcp, "services.tf"));
}

#[test]
fn services_tf_says_the_zone_is_on_and_billed() {
    let services = file(SPEC, Cloud::Aws, "services.tf");
    assert!(services.contains("`dns.private` is on: a spec with services turns it on"), "{services}");
    assert!(services.contains("Route 53 private hosted zone, about $0.50 a month"), "{services}");
}

#[test]
fn the_facts_a_cloud_issues_are_delivered_after_the_clients_set_up() {
    let services = file(SPEC, Cloud::Aws, "services.tf");
    assert!(services.contains("resource \"terraform_data\" \"isoloom_service_files_to_web\""), "{services}");
    assert!(
        services.contains("depends_on       = [terraform_data.isoloom_ready_web]") || services.contains("depends_on = [terraform_data.isoloom_ready_web]"),
        "{services}"
    );
    assert!(services.contains("content     = aws_iam_access_key.isoloom_service_files.secret"), "{services}");
    assert!(services.contains("/etc/isoloom/services/files/secret-key"), "{services}");
    // The identity client gets no key, its identity instead.
    let worker = services
        .split("\"isoloom_service_files_to_worker\"")
        .nth(1)
        .unwrap()
        .split("\nresource ")
        .next()
        .unwrap();
    assert!(
        !worker.contains("secret-key") && worker.contains("aws_iam_role.isoloom_client_worker.arn"),
        "{worker}"
    );
}

#[test]
fn an_identity_client_runs_as_its_identity_in_main_tf() {
    let aws = file(SPEC, Cloud::Aws, "main.tf");
    let worker = aws
        .split("resource \"aws_instance\" \"worker\"")
        .nth(1)
        .unwrap()
        .split("\nresource ")
        .next()
        .unwrap();
    assert!(
        worker.contains("iam_instance_profile = aws_iam_instance_profile.isoloom_client_worker.name"),
        "{worker}"
    );
    let web = aws
        .split("resource \"aws_instance\" \"web\"")
        .nth(1)
        .unwrap()
        .split("\nresource ")
        .next()
        .unwrap();
    assert!(!web.contains("iam_instance_profile"), "only the identity client");
    assert!(file(SPEC, Cloud::Azure, "main.tf").contains("type         = \"UserAssigned\""));
    assert!(file(SPEC, Cloud::Gcp, "main.tf").contains("google_service_account.isoloom_client_worker.email"));
}

#[test]
fn the_password_is_the_secret_variable_the_client_reads() {
    let main = file(SPEC, Cloud::Aws, "main.tf");
    assert!(main.contains("variable \"ISOLOOM_SECRET_DB_PASSWORD\""), "the client's file declares it");
    let services = file(SPEC, Cloud::Aws, "services.tf");
    assert!(services.contains("= var.ISOLOOM_SECRET_DB_PASSWORD"), "{services}");
    assert!(!services.contains("variable \"ISOLOOM_SECRET_DB_PASSWORD\""), "declared once, in main.tf");
    assert!(
        services.contains("variable \"ISOLOOM_SECRET_BUS_PASSWORD\""),
        "MSK's login, which no machine's file reads"
    );
}

#[test]
fn a_published_database_is_allow_listed_by_its_input() {
    let published = SPEC.replace("secrets:\n", "inputs:\n  PLAYER_IP: { sensitive: false }\nsecrets:\n").replace(
        "    clients: { web: { env_prefix: DATABASE } }\n",
        "    clients: { web: { env_prefix: DATABASE } }\n    publish: { allow: PLAYER_IP }\n",
    );
    let aws = file(&published, Cloud::Aws, "services.tf");
    assert!(
        aws.contains("publicly_accessible    = true") && aws.contains("var.PLAYER_IP") && aws.contains("variable \"PLAYER_IP\""),
        "{aws}"
    );
    assert!(file(&published, Cloud::Azure, "services.tf").contains("azurerm_postgresql_flexible_server_firewall_rule"));
    assert!(file(&published, Cloud::Gcp, "services.tf").contains("authorized_networks"));
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
    let rds = services.split("\"isoloom_service_db\"").nth(1).unwrap().split("\nresource ").next().unwrap();
    assert!(
        rds.contains("\"10.30.0.0/24\", \"10.32.0.0/24\""),
        "its network and the one reach lets in: {rds}"
    );
    assert!(!rds.contains("10.33.0.0/24"), "a client's other network gives no access: {rds}");
}
