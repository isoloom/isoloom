use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::CloudVmBackend;
use crate::cloud::Cloud;
use crate::testing::environment;

/// A shop: its web machine, its database, and a bucket only the module's outputs give.
const SPEC: &str = "version: 1\nname: shop\ndns: { private: true }\nnetworks: { lab: { cidr: 10.30.0.0/24 } }\nsecrets:\n  db-password: { env: SHOP_DB_PASSWORD }\n  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, app: { steps: [{ packages: [nginx] }] } }\nservices:\n  db:\n    kind: postgres\n    version: \"16\"\n    networks: { lab: 40 }\n    database: shop\n    user: shop\n    password: db-password\n    clients: { web: { env_prefix: DATABASE } }\n  files:\n    kind: object-storage\n    bucket: uploads\n    api: platform\n    versioning: true\n    networks: { lab: 41 }\n    credentials: { access_key: files-key, secret_key: files-secret }\n";

fn services_tf(yaml: &str, cloud: Cloud) -> String {
    let files = CloudVmBackend.render(&environment(yaml, Target::CloudVm)).unwrap();
    let path = format!(".isoloom/cloud-vm/{}/services.tf", cloud.id());
    files.into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}")).contents
}

#[test]
fn aws_renders_rds_and_s3_beside_the_module() {
    insta::assert_snapshot!(services_tf(SPEC, Cloud::Aws));
}

#[test]
fn azure_renders_a_flexible_server_and_a_storage_container() {
    insta::assert_snapshot!(services_tf(SPEC, Cloud::Azure));
}

#[test]
fn google_cloud_renders_cloud_sql_and_a_bucket() {
    insta::assert_snapshot!(services_tf(SPEC, Cloud::Gcp));
}

#[test]
fn the_password_is_the_secret_variable_the_client_reads() {
    let files = CloudVmBackend.render(&environment(SPEC, Target::CloudVm)).unwrap();
    let main = files.iter().find(|f| f.path == ".isoloom/cloud-vm/aws/main.tf").unwrap();
    assert!(
        main.contents.contains("variable \"ISOLOOM_SECRET_DB_PASSWORD\""),
        "the client's file declares it"
    );
    let services = services_tf(SPEC, Cloud::Aws);
    assert!(services.contains("= var.ISOLOOM_SECRET_DB_PASSWORD"), "{services}");
    assert!(!services.contains("variable \"ISOLOOM_SECRET_DB_PASSWORD\""), "declared once, in main.tf");
}

#[test]
fn the_clouds_without_services_refuse_and_azure_refuses_s3() {
    let env = environment(SPEC, Target::CloudVm);
    let refusals = CloudVmBackend.refusals(&env);
    let refused = |c: Cloud| refusals.iter().find(|(x, _)| *x == c).and_then(|(_, why)| why.clone());
    assert!(refused(Cloud::Linode).is_some_and(|w| w.contains("managed services")), "{refusals:?}");
    assert_eq!(refused(Cloud::Aws), None);
    let s3 = environment(&SPEC.replace("    api: platform\n", ""), Target::CloudVm);
    let azure = crate::paas::refusal(Cloud::Azure, &s3);
    assert!(azure.is_some_and(|w| w.contains("no S3 API")));
}
