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
    // The apt family installs from Docker's own repository with the signing key pinned by SHA-256
    // and the engine resolved from the repo; get.docker.com is only the RPM fallback (the `else`
    // branch), once. Compose is pinned either way.
    assert!(setup.contains("signed-by=/etc/apt/keyrings/docker.asc"), "{setup}");
    assert!(
        setup.contains("apt-cache madison docker-ce") && setup.contains("docker-compose-plugin"),
        "{setup}"
    );
    assert_eq!(setup.matches("get.docker.com").count(), 1, "{setup}");
    assert!(
        setup.find("get.docker.com") > setup.find("apt-cache madison docker-ce"),
        "get.docker.com is the RPM fallback: {setup}"
    );
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
    assert_eq!(
        why,
        "machine `w`: arm64, and every cloud's Docker host is x86-64 (its module picks x86-64 sizes and images)"
    );
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

/// One container asking for `disk_gb` of disk and `memory_mb` of memory.
fn one_container(memory_mb: u32, disk_gb: u32) -> Environment {
    env(&format!(
        "version: 1\nname: big\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w:\n    networks: {{ lab: 10 }}\n    resources: {{ cpus: 1, memory_mb: {memory_mb}, disk_gb: {disk_gb} }}\n    app: {{ image: nginx }}\n"
    ))
}

#[test]
fn a_cloud_whose_size_is_its_disk_too_sizes_the_host_by_its_table() {
    // 100 GB of containers (and Docker's own): past the 80 GB of the 4 GB droplet and plan.
    let tf = |cloud| cloud_module(cloud, &one_container(1024, 100)).unwrap().contents;
    assert!(tf(Cloud::DigitalOcean).contains("default = \"s-4vcpu-8gb\""), "{}", tf(Cloud::DigitalOcean));
    assert!(tf(Cloud::Linode).contains("default = \"g6-standard-4\""), "{}", tf(Cloud::Linode));
}

#[test]
fn a_cloud_whose_largest_host_lacks_the_disk_is_left_out_and_the_others_kept() {
    let e = one_container(1024, 400);
    let refused: Vec<(Cloud, String)> = super::refusals(&e).into_iter().filter_map(|(c, why)| Some((c, why?))).collect();
    assert_eq!(refused.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Cloud::DigitalOcean, Cloud::Linode]);
    assert!(
        refused[0]
            .1
            .contains("the containers' host asks for a 400 GB disk: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 320 GB"),
        "{}",
        refused[0].1
    );
    assert_eq!(refusal(&e), None, "the other clouds host them");
    let paths: Vec<String> = modules(&e).unwrap().into_iter().map(|f| f.path).collect();
    assert!(
        paths.iter().any(|p| p.contains("/aws/")) && !paths.iter().any(|p| p.contains("/digitalocean/") || p.contains("/linode/")),
        "{paths:?}"
    );
}

#[test]
fn no_cloud_hosting_the_containers_refuses_the_target() {
    let e = one_container(20480, 40);
    let why = refusal(&e).unwrap_or_default();
    assert!(
        why.starts_with("no cloud can run it (aws, azure, gcp, oci: the containers' host needs 21504 MB of memory"),
        "{why}"
    );
    assert!(why.contains("; digitalocean: the containers' host asks for 21 GB of memory"), "{why}");
}
