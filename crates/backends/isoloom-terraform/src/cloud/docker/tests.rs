//! The `cloud-docker` modules on small plans: the ports on every cloud, the set-up in order, a
//! module per cloud, the refusals.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use super::{Cloud, module as cloud_module, modules, refusal};
use crate::testing::environment;

/// Two containers, publishing TCP and UDP ports.
pub(crate) const SPEC: &str = "
version: 1
name: dock
inputs: [TOKEN]
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    ports: { web: { port: 80, publish: 8080 }, dns: { port: 53, protocol: udp, publish: 5353 } }
    resources: { cpus: 2, memory_mb: 4096, disk_gb: 30 }
    app: { image: nginx }
  db:
    networks: { lab: 11 }
    ports: { db: { port: 5432, publish: 15432 } }
    app: { image: postgres }
";

/// A plan of `yaml` on `cloud-docker`.
pub(crate) fn env(yaml: &str) -> Environment {
    environment(yaml, Target::CloudDocker)
}

fn module(cloud: Cloud) -> String {
    cloud_module(cloud, &env(SPEC)).unwrap().contents
}

#[test]
fn every_cloud_opens_ssh_and_the_published_ports() {
    let aws = module(Cloud::Aws);
    assert!(aws.contains("description = \"published 5353/udp\""), "{aws}");
    assert!(module(Cloud::Azure).contains("destination_port_ranges    = [\"22\", \"8080\", \"15432\"]"));
    assert!(module(Cloud::Gcp).contains("ports    = [\"5353\"]"));
    assert_eq!(module(Cloud::DigitalOcean).matches("inbound_rule {").count(), 4);
    assert!(module(Cloud::Linode).contains("ports    = \"22,8080,15432\""));
    assert!(module(Cloud::Oci).contains("protocol = \"17\""));
}

#[test]
fn the_module_is_infrastructure_only_and_the_host_installs_docker_from_its_setup_script() {
    // Terraform makes only the VM: the runner pipes the shared setup.sh into the host over SSH to
    // install Docker, then delivers the project and starts the Compose file (the run manifest's
    // own steps). Nothing in the module reaches the guest.
    let files = super::setup(&env(SPEC)).unwrap();
    let setup = &files
        .iter()
        .find(|f| f.path.ends_with("cloud-docker/aws/setup.sh"))
        .expect("a setup.sh")
        .contents;
    assert!(setup.contains("get.docker.com") && setup.contains("docker-compose-plugin"), "{setup}");
    for cloud in Cloud::ALL {
        let tf = module(cloud);
        for gone in [
            "terraform_data\" \"environment\"",
            "remote-exec",
            "local-exec",
            "ssh_private_key_file",
            "local_file",
        ] {
            assert!(!tf.contains(gone), "{cloud:?} still has {gone}");
        }
    }
    // The secret input never travels in the user data; the module takes none in a file either.
    let tf = module(Cloud::Aws);
    assert!(!tf.contains("lookup(var.inputs, \"TOKEN\""), "{tf}");
}

#[test]
fn each_cloud_hands_the_set_up_to_its_own_first_boot() {
    let cases = [
        (Cloud::Aws, "user_data_base64", "base64gzip("),
        (Cloud::Azure, "custom_data", "base64encode("),
        (Cloud::Gcp, "startup-script", ""),
        (Cloud::DigitalOcean, "user_data", ""),
        (Cloud::Linode, "user_data", "base64encode("),
        (Cloud::Oci, "user_data", "base64encode("),
    ];
    for (cloud, key, wrap) in cases {
        let tf = module(cloud);
        let line = tf
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{key} ")))
            .unwrap_or_else(|| panic!("no {key} in {cloud:?}: {tf}"));
        assert!(line.contains(&format!("= {wrap}\"#!/bin/sh\\n")), "{cloud:?}: {line}");
    }
}

#[test]
fn arm64_is_refused_from_the_plan() {
    let e = env(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    app: { image: nginx }\n",
    );
    let why = refusal(&e).unwrap();
    assert_eq!(why, "machine `w`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)");
    assert!(modules(&e).unwrap_err().to_string().ends_with(&why));
}

#[test]
fn a_module_per_cloud() {
    let files = modules(&env(SPEC)).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for cloud in Cloud::ALL {
        assert!(paths.contains(&format!(".isoloom/cloud-docker/{}/main.tf", cloud.id()).as_str()), "{paths:?}");
    }
}
