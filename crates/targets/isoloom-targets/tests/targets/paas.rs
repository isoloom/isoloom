//! The platform level end to end (`services:`, the `paas` feature): the shop example's files on a
//! container target, a server and the clouds, pinned as snapshots; its services in the resolved
//! output; and a spec of services alone.

use isoloom_spec::Target;
use isoloom_targets::generate;
use isoloom_test_support::{hcl_body, hcl_names, hcl_one, hcl_text, manifest_names, yaml, yaml_documents, yaml_str, yaml_strs};

use crate::testing::{Outcome, file, valid};

const SHOP: &str = include_str!("../../../../../examples/paas/shop/isoloom.yml");
const DATA_ONLY: &str = include_str!("../../../../../examples/paas/data-only/isoloom.yml");

/// The line of a Dockerfile that downloads `what` (`isoloom_file '<path>' … 'download' '<url>'`).
fn download<'a>(dockerfile: &'a str, what: &str) -> Option<&'a str> {
    dockerfile
        .lines()
        .find(|l| l.starts_with("RUN ") && l.contains(&format!("isoloom_file '{what}'")))
}

#[test]
fn compose_runs_each_service_as_a_service_of_the_project() -> Outcome {
    let files = generate(&valid(SHOP)?, Target::Docker)?;
    let compose = file(&files, ".isoloom/docker/compose.yml")?;
    let parsed = yaml(compose);
    // The catalogue's postgres: built on the official image, its data in a volume.
    let db = file(&files, ".isoloom/docker/build/db/Dockerfile")?;
    let from = db.lines().find(|l| l.starts_with("FROM ")).unwrap_or_default();
    assert!(from.starts_with("FROM docker.io/library/postgres:16-bookworm@sha256:"), "{from}");
    assert_eq!(
        yaml_strs(&parsed, "services.db.volumes").first().map(String::as_str),
        Some("db-data:/var/lib/postgresql/data")
    );
    // The shop reads each service's facts from its environment.
    for (key, value) in [
        ("DATABASE_HOST", "db.shop.internal"),
        ("STORAGE_BUCKET_FILE", "/etc/isoloom/services/files/bucket"),
        ("STORAGE_WAIT", "/etc/isoloom/services/wait-for"),
    ] {
        assert_eq!(yaml_str(&parsed, &format!("services.web.environment.{key}")), value, "{key}");
    }
    // SeaweedFS and Kafka come from their pinned releases, in images Isoloom builds.
    for (service, archive, pinned) in [
        ("files", "/opt/isoloom/seaweedfs.tar.gz", "seaweedfs/releases/download/4.53/"),
        ("bus", "/opt/isoloom/kafka.tgz", "/kafka_2.13-4.3.2.tgz"),
    ] {
        let dockerfile = file(&files, &format!(".isoloom/docker/build/{service}/Dockerfile"))?;
        let line = download(dockerfile, archive).unwrap_or_default();
        assert!(line.contains(pinned), "{service} downloads {pinned}: {line}");
    }
    insta::assert_snapshot!("shop_docker_compose", compose);
    Ok(())
}

#[test]
fn proxmox_runs_each_service_as_a_debian_vm() -> Outcome {
    let l = valid(SHOP)?;
    assert!(l.refusal(Target::Proxmox).is_none(), "{:?}", l.refusal(Target::Proxmox));
    let files = generate(&l, Target::Proxmox)?;
    let main = file(&files, ".isoloom/proxmox/main.tf")?;
    let vms = hcl_names(&hcl_body(main), "resource", "proxmox_virtual_environment_vm");
    for vm in ["db", "files", "bus"] {
        assert!(vms.iter().any(|v| v == vm), "{vm} is a VM of the module: {vms:?}");
    }
    insta::assert_snapshot!("shop_proxmox_main_tf", main);
    Ok(())
}

#[test]
fn the_clouds_render_their_managed_services_beside_the_machines() -> Outcome {
    let files = generate(&valid(SHOP)?, Target::CloudVm)?;
    for cloud in ["aws", "azure", "gcp"] {
        let services = file(&files, &format!(".isoloom/cloud-vm/{cloud}/services.tf"))?;
        insta::assert_snapshot!(format!("shop_cloud_vm_{cloud}_services_tf"), services);
    }
    assert!(
        !files.iter().any(|f| f.path.starts_with(".isoloom/cloud-vm/linode/")),
        "Linode refuses services"
    );
    let aws = hcl_body(file(&files, ".isoloom/cloud-vm/aws/main.tf")?);
    let instances = hcl_names(&aws, "resource", "aws_instance");
    assert!(!instances.iter().any(|i| i == "db"), "the database is RDS, not a VM: {instances:?}");
    Ok(())
}

#[test]
fn the_resolved_output_carries_the_services_facts() -> Outcome {
    let files = generate(&valid(SHOP)?, Target::Docker)?;
    let resolved: serde_json::Value = serde_json::from_str(file(&files, ".isoloom/resolved.json")?)?;
    let db = &resolved["services"]["db"];
    assert_eq!(db["host"], "db.shop.internal");
    assert_eq!(db["targets"]["cloud-vm"], "managed");
    assert_eq!(db["targets"]["docker"], "machine");
    assert_eq!(db["published"]["allow_input"], "PLAYER_IP");
    assert_eq!(resolved["services_zone"]["turned_on_by_services"], true);
    assert_eq!(resolved["services"]["files"]["clients"]["web"]["environment"]["STORAGE_AUTH"], "keys");
    let wait = &resolved["services"]["bus"]["clients"]["web"]["wait"];
    assert_eq!(wait["helper"], "/etc/isoloom/services/wait-for", "{wait}");
    assert_eq!(wait["default_timeout_s"], 300, "{wait}");
    insta::assert_snapshot!("shop_resolved_services", serde_json::to_string_pretty(&resolved["services"])?);
    Ok(())
}

#[test]
fn a_spec_of_services_alone_generates_and_publishes_its_database() -> Outcome {
    let l = valid(DATA_ONLY)?;
    let docker = generate(&l, Target::Docker)?;
    let compose = yaml(file(&docker, ".isoloom/docker/compose.yml")?);
    let ports = yaml_strs(&compose, "services.db.ports");
    assert!(ports.iter().any(|p| p.ends_with(":5432")), "the database's port is published: {ports:?}");
    let cloud = generate(&l, Target::CloudVm)?;
    let aws = hcl_body(file(&cloud, ".isoloom/cloud-vm/aws/services.tf")?);
    assert_eq!(hcl_text(hcl_one(&aws, "aws_db_instance", "db"), "publicly_accessible"), "true");
    let ingress = hcl_text(
        hcl_one(&aws, "aws_security_group", "isoloom_service_db")
            .blocks()
            .find(|b| b.identifier() == "ingress")
            .ok_or("no ingress")?
            .body(),
        "cidr_blocks",
    );
    assert!(ingress.contains("var.PLAYER_IP"), "published to the player alone: {ingress}");
    Ok(())
}

/// Writes the shop's and the services-only spec's files for the container targets (Compose and
/// Kubernetes), a server and the clouds into `ISOLOOM_PAAS_OUT/<example>/`, for the formats' own validators (`docker
/// compose config`, `kubeconform`, `terraform validate`), run by hand:
/// `ISOLOOM_PAAS_OUT=<dir> cargo test -p isoloom-targets --features paas --test targets paas:: -- --ignored`.
#[test]
#[ignore = "writes files for the validators, run by hand"]
fn write_the_examples_for_the_validators() -> Outcome {
    let Some(out) = std::env::var_os("ISOLOOM_PAAS_OUT") else {
        return Ok(());
    };
    for (name, yaml) in [("shop", SHOP), ("data-only", DATA_ONLY)] {
        let dir = std::path::PathBuf::from(&out).join(name);
        let l = valid(yaml)?;
        for target in [Target::Docker, Target::Kubernetes, Target::Proxmox, Target::CloudVm] {
            for f in generate(&l, target)? {
                let path = dir.join(&f.path);
                std::fs::create_dir_all(path.parent().ok_or("a file in a folder")?)?;
                std::fs::write(path, f.contents)?;
            }
        }
    }
    Ok(())
}

#[test]
fn kubernetes_generates_the_services_as_workloads_reached_at_their_service_names() -> Outcome {
    let l = valid(SHOP)?;
    assert_eq!(
        l.for_target(Target::Kubernetes).machines["web"]
            .environment
            .get("DATABASE_HOST")
            .map(String::as_str),
        Some("db"),
        "a cluster's own Service name: Kubernetes takes no zone of Isoloom's"
    );
    let files = generate(&l, Target::Kubernetes)?;
    let docs = yaml_documents(file(&files, ".isoloom/kubernetes/environment.yaml")?);
    let deployments = manifest_names(&docs, "Deployment");
    let services = manifest_names(&docs, "Service");
    for name in ["db", "files", "bus"] {
        assert!(deployments.iter().any(|d| d == name), "{name} runs as a Deployment: {deployments:?}");
        assert!(services.iter().any(|s| s == name), "{name} is reached at its Service's name: {services:?}");
    }
    Ok(())
}
