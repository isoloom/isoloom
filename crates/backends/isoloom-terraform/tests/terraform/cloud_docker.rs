//! The cloud-docker modules of hello-stack on every cloud, rendered from their plan, as
//! snapshots; and their determinism.

use crate::testing::{Outcome, docker_host, example, file};
use isoloom_plan::Environment;
use isoloom_render::backend::Files;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_docker_modules;
use isoloom_test_support::{hcl_resource, hcl_values};

/// The cloud-docker modules of `e`, its host starting the Compose file.
fn modules(e: &Environment) -> isoloom_terraform::Result<Files> {
    cloud_docker_modules(e, &docker_host(&[]))
}

/// Example `name`'s plan for `target`.
fn env(name: &str, target: Target) -> Outcome<Environment> {
    crate::testing::planned(&example(name)?, target)
}

/// hello-stack's module on `cloud`.
fn cloud(cloud: &str) -> Outcome<String> {
    let files = modules(&env("hello-stack", Target::CloudDocker)?)?;
    Ok(file(&files, &format!("cloud-docker/{cloud}/main.tf"))?.to_owned())
}

/// The module a reviewer reads as a file: hello-stack's host on each cloud, its Compose file
/// started at boot.
#[test]
fn hello_stacks_host_on_each_cloud() -> Outcome {
    for id in ["aws", "azure", "gcp", "digitalocean", "linode", "oci"] {
        insta::assert_snapshot!(format!("cloud_docker_{id}_hello_stack"), cloud(id)?);
    }
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "udp-dns", "segmented", "workbench"] {
        let e = env(name, Target::CloudDocker)?;
        assert_eq!(modules(&e)?, modules(&e)?, "{name}");
    }
    Ok(())
}

#[test]
fn every_clouds_host_has_the_disk_its_containers_need() -> Outcome {
    let disk = isoloom_render::container::HostSize::of(&env("hello-stack", Target::CloudDocker)?).disk_gb;
    // DigitalOcean's and Linode's disks come with the size the host's variable picks.
    for (id, kind, key, size) in [
        ("aws", "aws_instance", "volume_size", disk),
        ("azure", "azurerm_linux_virtual_machine", "disk_size_gb", disk),
        ("gcp", "google_compute_instance", "size", disk),
        // Oracle Cloud's boot volumes start at 50 GB.
        ("oci", "oci_core_instance", "boot_volume_size_in_gbs", disk.max(50)),
    ] {
        let host = hcl_resource(&cloud(id)?, kind, "env");
        assert_eq!(hcl_values(&host, key), [size.to_string()], "{id}");
    }
    Ok(())
}
