//! The platform level end to end (`services:`, the `paas` feature): the shop example's files on a
//! container target, a server and the clouds, pinned as snapshots; its services in the resolved
//! output; and a spec of services alone.

#![cfg(feature = "paas")]

use isoloom_spec::Target;
use isoloom_targets::{GeneratedFile, Lab, generate};

const SHOP: &str = include_str!("../../../../examples/paas/shop/isoloom.yml");
const DATA_ONLY: &str = include_str!("../../../../examples/paas/data-only/isoloom.yml");

fn lab(yaml: &str) -> Lab {
    let lab = Lab::parse(yaml).unwrap();
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
fn compose_runs_each_service_as_a_service_of_the_project() {
    let files = generate(&lab(SHOP), Target::Docker).unwrap();
    let compose = file(&files, ".isoloom/docker/compose.yml");
    // The catalogue's postgres: built on the official image, its data in a volume.
    assert!(
        files.iter().any(|f| f.contents.contains("postgres:16-bookworm")),
        "no file builds on the official image"
    );
    assert!(compose.contains("DATABASE_HOST: db.shop.internal"), "{compose}");
    assert!(compose.contains("STORAGE_BUCKET_FILE: /etc/isoloom/services/files/bucket"), "{compose}");
    // SeaweedFS and Kafka come from their pinned releases, in images Isoloom builds.
    for pinned in ["seaweedfs/releases/download/4.53", "kafka_2.13-4.3.2.tgz"] {
        assert!(files.iter().any(|f| f.contents.contains(pinned)), "{pinned}");
    }
    insta::assert_snapshot!("shop_docker_compose", compose);
}

#[test]
fn proxmox_runs_each_service_as_a_debian_vm() {
    let l = lab(SHOP);
    assert!(l.refusal(Target::Proxmox).is_none(), "{:?}", l.refusal(Target::Proxmox));
    let files = generate(&l, Target::Proxmox).unwrap();
    let main = file(&files, ".isoloom/proxmox/main.tf");
    for vm in ["\"db\"", "\"files\"", "\"bus\""] {
        assert!(main.contains(vm), "{vm} is a VM of the module");
    }
    insta::assert_snapshot!("shop_proxmox_main_tf", main);
}

#[test]
fn the_clouds_render_their_managed_services_beside_the_machines() {
    let files = generate(&lab(SHOP), Target::CloudVm).unwrap();
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

#[test]
fn the_resolved_output_carries_the_services_facts() {
    let files = generate(&lab(SHOP), Target::Docker).unwrap();
    let resolved: serde_json::Value = serde_json::from_str(file(&files, ".isoloom/resolved.json")).unwrap();
    let db = &resolved["services"]["db"];
    assert_eq!(db["host"], "db.shop.internal");
    assert_eq!(db["targets"]["cloud-vm"], "managed");
    assert_eq!(db["targets"]["docker"], "machine");
    assert_eq!(db["published"]["allow_input"], "PLAYER_IP");
    assert_eq!(resolved["services_zone"]["turned_on_by_services"], true);
    assert_eq!(resolved["services"]["files"]["clients"]["web"]["environment"]["STORAGE_AUTH"], "keys");
    insta::assert_snapshot!("shop_resolved_services", serde_json::to_string_pretty(&resolved["services"]).unwrap());
}

#[test]
fn a_spec_of_services_alone_generates_and_publishes_its_database() {
    let l = lab(DATA_ONLY);
    let docker = generate(&l, Target::Docker).unwrap();
    let compose = file(&docker, ".isoloom/docker/compose.yml");
    assert!(compose.contains("5432"), "{compose}");
    let cloud = generate(&l, Target::CloudVm).unwrap();
    let aws = file(&cloud, ".isoloom/cloud-vm/aws/services.tf");
    assert!(aws.contains("publicly_accessible    = true") && aws.contains("var.PLAYER_IP"), "{aws}");
}

/// Writes the shop's and the services-only spec's files for the container target, a server and
/// the clouds into `ISOLOOM_PAAS_OUT/<example>/`, for the formats' own validators (`docker
/// compose config`, `terraform validate`), run by hand:
/// `ISOLOOM_PAAS_OUT=<dir> cargo test -p isoloom-targets --features paas --test paas -- --ignored`.
#[test]
#[ignore = "writes files for the validators, run by hand"]
fn write_the_examples_for_the_validators() {
    let Some(out) = std::env::var_os("ISOLOOM_PAAS_OUT") else { return };
    for (name, yaml) in [("shop", SHOP), ("data-only", DATA_ONLY)] {
        let dir = std::path::PathBuf::from(&out).join(name);
        let l = lab(yaml);
        for target in [Target::Docker, Target::Proxmox, Target::CloudVm] {
            for f in generate(&l, target).unwrap() {
                let path = dir.join(&f.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, f.contents).unwrap();
            }
        }
    }
}

#[test]
fn kubernetes_generates_the_services_as_workloads_reached_at_their_service_names() {
    let l = lab(SHOP);
    assert_eq!(
        l.for_target(Target::Kubernetes).machines["web"]
            .environment
            .get("DATABASE_HOST")
            .map(String::as_str),
        Some("db"),
        "a cluster's own Service name: Kubernetes takes no zone of Isoloom's"
    );
    let files = generate(&l, Target::Kubernetes).unwrap();
    for name in ["db", "files", "bus"] {
        assert!(
            files
                .iter()
                .any(|f| f.path.starts_with(".isoloom/kubernetes/") && f.contents.contains(&format!("name: {name}"))),
            "{name}"
        );
    }
}
