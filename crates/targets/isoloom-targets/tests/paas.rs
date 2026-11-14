//! The platform level end to end (`services:`, the `paas` feature): the shop example's files on a
//! container target, a VM target and the clouds, pinned as snapshots, and the targets that refuse
//! a service saying why.

#![cfg(feature = "paas")]

use isoloom_spec::Target;
use isoloom_targets::{GeneratedFile, Lab, generate};

const SHOP: &str = include_str!("../../../../examples/paas/shop/isoloom.yml");

fn shop() -> Lab {
    let lab = Lab::parse(SHOP).unwrap();
    assert!(lab.problems().is_empty(), "{:?}", lab.problems());
    lab
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    files
        .iter()
        .find(|f| f.path == path)
        .map_or_else(|| panic!("no {path} in {paths:?}"), |f| f.contents.as_str())
}

#[test]
fn compose_runs_postgres_and_the_s3_server_as_services_of_the_project() {
    let files = generate(&shop(), Target::Docker).unwrap();
    let compose = file(&files, ".isoloom/docker/compose.yml");
    // The catalogue's postgres: built on the official image, its data in a volume.
    assert!(
        files.iter().any(|f| f.contents.contains("postgres:16-bookworm")),
        "no file builds on the official image"
    );
    assert!(compose.contains("db-data:/var/lib/postgresql/data"), "{compose}");
    assert!(compose.contains("chrislusf/seaweedfs:4.53"), "{compose}");
    assert!(compose.contains("DATABASE_HOST: db.shop.internal"), "{compose}");
    insta::assert_snapshot!("shop_docker_compose", compose);
}

#[test]
fn proxmox_runs_postgres_as_a_debian_vm_and_refuses_the_bucket() {
    let lab = shop();
    let refused = lab.refusal(Target::Proxmox).unwrap();
    assert_eq!(refused.path.to_string(), "services.files");
    let postgres_only = SHOP
        .split("  # Read from outside")
        .next()
        .unwrap()
        .replace("  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\n", "");
    let lab = Lab::parse(&postgres_only).unwrap();
    assert!(lab.problems().is_empty(), "{:?}", lab.problems());
    let files = generate(&lab, Target::Proxmox).unwrap();
    let main = file(&files, ".isoloom/proxmox/main.tf");
    assert!(main.contains("\"db\""), "the database is a VM of the module");
    insta::assert_snapshot!("shop_proxmox_main_tf", main);
}

#[test]
fn the_clouds_render_their_managed_services_beside_the_machines() {
    let files = generate(&shop(), Target::CloudVm).unwrap();
    for cloud in ["aws", "azure", "gcp"] {
        let services = file(&files, &format!(".isoloom/cloud-vm/{cloud}/services.tf"));
        insta::assert_snapshot!(format!("shop_cloud_vm_{cloud}_services_tf"), services);
    }
    assert!(
        !files.iter().any(|f| f.path.starts_with(".isoloom/cloud-vm/linode/")),
        "Linode refuses services"
    );
    let aws = file(&files, ".isoloom/cloud-vm/aws/main.tf");
    assert!(!aws.contains("resource \"aws_instance\" \"db\""), "the database is RDS, not a VM");
}

/// Writes the shop's files for the container target and the clouds into `ISOLOOM_PAAS_OUT`, for
/// the formats' own validators (`docker compose config`, `terraform validate`), run by hand:
/// `ISOLOOM_PAAS_OUT=<dir> cargo test -p isoloom-targets --features paas --test paas -- --ignored`.
#[test]
#[ignore = "writes files for the validators, run by hand"]
fn write_the_shop_for_the_validators() {
    let Some(out) = std::env::var_os("ISOLOOM_PAAS_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    let lab = shop();
    for target in [Target::Docker, Target::CloudVm] {
        for f in generate(&lab, target).unwrap() {
            let path = out.join(&f.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, f.contents).unwrap();
        }
    }
}
