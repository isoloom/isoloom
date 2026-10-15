//! The Docker host on small plans: its size, its ports on every cloud, its set-up in order.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use super::host::{Sizing, published};
use super::{Cloud, check, modules, refusal};

const SPEC: &str = "
version: 1
name: dock
inputs: [TOKEN]
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    ports: [{ port: 80, publish: 8080 }, { port: 53, protocol: udp, publish: 5353 }]
    resources: { cpus: 2, memory_mb: 4096, disk_gb: 30 }
    docker: { image: nginx }
  db:
    networks: { lab: 11 }
    ports: [{ port: 5432, publish: 15432 }]
    docker: { image: postgres }
";

fn env(yaml: &str) -> Environment {
    let spec = isoloom_spec::parse(yaml).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::CloudDocker, None)
}

fn module(cloud: Cloud) -> String {
    cloud.render(&env(SPEC)).unwrap().contents
}

#[test]
fn the_host_is_sized_for_every_container_at_once() {
    let s = Sizing::of(&env(SPEC));
    assert_eq!(
        s,
        Sizing {
            cpus: 3,
            memory_mb: 4096 + 512 + 1024,
            disk_gb: 35
        }
    );
    assert_eq!(s.pick(["small", "medium", "large"]), "medium");
}

#[test]
fn published_ports_are_sorted_once_per_protocol() {
    let e = env(SPEC);
    assert_eq!(published(&e, isoloom_spec::model::Protocol::Tcp), [8080, 15432]);
    assert_eq!(published(&e, isoloom_spec::model::Protocol::Udp), [5353]);
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
fn the_set_up_waits_for_cloud_init_then_starts_compose_with_the_inputs() {
    let tf = module(Cloud::Linode);
    let at = |s: &str| tf.find(s).unwrap_or_else(|| panic!("{s} in {tf}"));
    assert!(at("cloud-init status --wait") < at("sudo shutdown -h +"));
    assert!(at("sudo shutdown -h +") < at("tar -xzf /tmp/isoloom-project.tgz"));
    assert!(at("get.docker.com") < at("set -a; . /tmp/isoloom-inputs.env; set +a;"));
    assert!(at("docker compose -f .isoloom/docker/compose.yml up -d --build --wait") < at("echo ready > /var/lib/isoloom/ready"));
    assert!(tf.contains("destination = \"/tmp/isoloom-inputs.env\""), "{tf}");
    // AWS schedules the auto-stop in its user data instead.
    assert!(!module(Cloud::Aws).contains("sudo shutdown"));
}

#[test]
fn arm64_is_refused_from_the_plan() {
    let e = env(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    arch: arm64\n    docker: { image: nginx }\n",
    );
    let why = refusal(&e).unwrap();
    assert_eq!(why, "machine `w`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)");
    assert!(check(&e).unwrap_err().to_string().ends_with(&why));
}

#[test]
fn a_module_per_cloud() {
    let files = modules(&env(SPEC)).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for cloud in Cloud::ALL {
        assert!(paths.contains(&format!(".isoloom/cloud-docker/{}/main.tf", cloud.id()).as_str()), "{paths:?}");
    }
}
