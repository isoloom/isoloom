//! The resolved snapshot agrees with the generators: every address it states is the one the
//! generated files use, and what it says about targets and checks is what `isoloom targets` and
//! the runners say.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{Target, load};
use isoloom_targets::generate;
use isoloom_targets::resolved::{lookup, resolve};

fn example(name: &str) -> isoloom_spec::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name)).expect("example parses")
}

fn contents(files: &[isoloom_targets::GeneratedFile], suffix: &str) -> String {
    files.iter().find(|f| f.path.ends_with(suffix)).map_or_else(
        || panic!("no {suffix} among {:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>()),
        |f| f.contents.clone(),
    )
}

#[test]
fn addresses_and_routers_match_the_generators() {
    let spec = example("segmented");
    let r = resolve(&spec);
    assert_eq!(lookup(&r, "resolved_version").unwrap(), 2);
    assert_eq!(lookup(&r, "machines.web.addresses.front").unwrap(), "10.61.10.10");
    assert_eq!(lookup(&r, "networks.back.router").unwrap(), "10.61.20.254");
    assert_eq!(lookup(&r, "networks.back.internet").unwrap(), false);
    assert_eq!(lookup(&r, "machines.cache.routes.0.via").unwrap(), "10.61.20.254");
    assert_eq!(lookup(&r, "machines.user.access").unwrap(), true);
    assert_eq!(lookup(&r, "start_order").unwrap(), &serde_json::json!(["cache", "web", "user"]));
    // Every address the snapshot states is in the files the generators write.
    let vagrant = contents(&generate(&spec, Target::Vagrant).unwrap(), "vagrant/Vagrantfile");
    let compose = contents(&generate(&spec, Target::Docker).unwrap(), "docker/compose.yml");
    for (_, m) in lookup(&r, "machines").unwrap().as_object().unwrap() {
        for a in m["addresses"].as_object().unwrap().values() {
            assert!(vagrant.contains(a.as_str().unwrap()), "{a} missing from the Vagrantfile");
        }
        for a in m["docker_addresses"].as_object().unwrap().values() {
            if m["shapes"].as_array().unwrap().iter().any(|s| s == "docker") {
                assert!(compose.contains(a.as_str().unwrap()), "{a} missing from the Compose file");
            }
        }
    }
    assert!(vagrant.contains(lookup(&r, "router.addresses.front").unwrap().as_str().unwrap()));
}

#[test]
fn targets_say_what_generates_and_why_the_others_dont() {
    let r = resolve(&example("segmented"));
    let generated = lookup(&r, "targets.generated").unwrap().as_array().unwrap();
    assert!(generated.iter().any(|t| t == "docker") && generated.iter().any(|t| t == "vagrant"));
    assert!(generated.iter().any(|t| t == "cloud-vm"));
    // pivot-dmz's Kali access machine has no cloud image: refused, with the reason.
    let p = resolve(&example("pivot-dmz"));
    assert!(lookup(&p, "targets.refused.cloud-vm").unwrap().as_str().unwrap().contains("kali"));
    assert!(
        lookup(&r, "targets.not_possible.hybrid")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("containers and others are VMs")
    );
}

#[test]
fn checks_are_listed_by_position_with_their_runner() {
    let r = resolve(&example("segmented"));
    assert_eq!(lookup(&r, "checks.default_position").unwrap(), "user");
    assert_eq!(lookup(&r, "checks.positions.0.runner").unwrap(), "isoloom-check");
    assert_eq!(lookup(&r, "checks.positions.1.runner").unwrap(), "isoloom-check-cache");
    assert_eq!(
        lookup(&r, "checks.positions.0.checks.0.name").unwrap(),
        "the web page answers through the router"
    );
    assert_eq!(lookup(&r, "checks.positions.0.checks.0.target").unwrap(), "http://web/");
    assert_eq!(lookup(&r, "checks.positions.0.checks.2.target").unwrap(), "10.61.20.20:6379");
}

#[test]
fn docker_blocks_and_a_gateway_are_resolved() {
    // air-gapped moves its 192.168 network onto a 10.x block on Docker: both are in the snapshot.
    let r = resolve(&example("air-gapped"));
    assert_eq!(lookup(&r, "networks.lab.cidr").unwrap(), "192.168.62.0/24");
    assert_eq!(lookup(&r, "networks.lab.docker_cidr").unwrap(), "10.62.0.0/24");
    assert_eq!(lookup(&r, "machines.app.addresses.lab").unwrap(), "192.168.62.10");
    assert_eq!(lookup(&r, "machines.app.docker_addresses.lab").unwrap(), "10.62.0.10");
    assert_eq!(lookup(&r, "machines.app.offline").unwrap(), true);
    assert!(lookup(&r, "router").unwrap().is_null());
    // edge-firewall: fw owns two networks; machines behind it route through it.
    let r = resolve(&example("edge-firewall"));
    assert_eq!(lookup(&r, "machines.fw.gateway_of").unwrap(), &serde_json::json!(["dmz", "lan"]));
    assert_eq!(lookup(&r, "machines.web.default_gateway").unwrap(), "10.70.10.1");
    assert_eq!(lookup(&r, "networks.dmz.gateway").unwrap(), "fw");
    assert!(lookup(&r, "networks.dmz.router").unwrap().is_null());
    assert_eq!(lookup(&r, "published").unwrap(), &serde_json::json!([]));
}

#[test]
fn the_snapshot_is_a_generated_file_of_every_target() {
    let spec = example("hello-stack");
    for t in [Target::Docker, Target::Vagrant, Target::Kubernetes] {
        let files = generate(&spec, t).unwrap();
        let snap = contents(&files, ".isoloom/resolved.json");
        assert!(snap.starts_with("{\n  \"generated_by_isoloom\""), "{snap}");
        assert!(snap.ends_with("}\n"));
    }
    let r = resolve(&spec);
    assert_eq!(lookup(&r, "published.0.host_port").unwrap(), 8080);
    assert_eq!(lookup(&r, "checks.positions.0.position").unwrap(), "networks");
    assert!(lookup(&r, "checks.positions.0.machine").unwrap().is_null());
    assert!(lookup(&r, "nothing.here").is_none());
}

#[test]
fn cloud_vm_lists_its_generated_and_refused_clouds() {
    let spec = example("windows-hello");
    let r = resolve(&spec);
    let clouds = lookup(&r, "targets.clouds.cloud-vm").unwrap();
    assert_eq!(clouds["generated"], serde_json::json!(["aws", "azure"]));
    let refused = clouds["refused"].as_object().unwrap();
    assert_eq!(refused.keys().collect::<Vec<_>>(), ["gcp", "digitalocean", "linode", "oci"]);
    assert!(refused["gcp"].as_str().unwrap().contains("Windows on GCP"));
    // What the snapshot says is what the generator writes: a module per generated cloud only.
    let files = generate(&spec, Target::CloudVm).unwrap();
    let written = |cloud: &str| files.iter().any(|f| f.path.starts_with(&format!(".isoloom/cloud-vm/{cloud}/")));
    for cloud in clouds["generated"].as_array().unwrap() {
        assert!(written(cloud.as_str().unwrap()), "{cloud}: no module");
    }
    for cloud in refused.keys() {
        assert!(!written(cloud), "{cloud}: refused, yet its module is written");
    }
}

#[test]
fn only_a_generated_cloud_vm_lists_its_clouds() {
    // pivot-dmz: cloud-vm is refused as a whole (AWS has no Kali image), so no clouds.
    let p = resolve(&example("pivot-dmz"));
    assert!(lookup(&p, "targets.refused.cloud-vm").is_some());
    assert_eq!(lookup(&p, "targets.clouds").unwrap(), &serde_json::json!({}));
    // segmented: every cloud-vm cloud is listed once, generated or refused; no other target is.
    let s = resolve(&example("segmented"));
    let clouds = lookup(&s, "targets.clouds").unwrap().as_object().unwrap();
    assert_eq!(clouds.keys().collect::<Vec<_>>(), ["cloud-vm"]);
    let generated = clouds["cloud-vm"]["generated"].as_array().unwrap().len();
    let refused = clouds["cloud-vm"]["refused"].as_object().unwrap().len();
    assert_eq!(generated + refused, 6);
}

#[test]
fn each_machine_records_its_resolved_os_and_its_ports() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { os: windows-server, networks: { lab: 5 }, ports: [{ port: 3389 }], vm: { provision: [a.ps1] } }\n  b: { networks: { lab: 6 }, docker: { image: x } }\n",
    )
    .unwrap();
    let r = resolve(&spec);
    assert_eq!(lookup(&r, "machines.a.os").unwrap(), "windows-server-2022");
    assert_eq!(lookup(&r, "machines.a.os_family").unwrap(), "windows-server");
    assert_eq!(lookup(&r, "machines.a.os_version").unwrap(), "2022", "the pinned default, recorded");
    assert_eq!(lookup(&r, "machines.a.ports.0.port").unwrap(), 3389);
    assert!(lookup(&r, "machines.b.os_family").is_none(), "a container without an `os` has none");
}
