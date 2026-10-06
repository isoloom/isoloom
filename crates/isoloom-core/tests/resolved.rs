//! The resolved snapshot agrees with the generators: every address it states is the one the
//! generated files use, and what it says about targets and checks is what `isoloom targets` and
//! the runners say.

use std::path::Path;

use isoloom_core::resolved::{lookup, resolve};
use isoloom_core::{Target, generate, load};

fn example(name: &str) -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).expect("example parses")
}

fn contents(files: &[isoloom_core::GeneratedFile], suffix: &str) -> String {
    files
        .iter()
        .find(|f| f.path.ends_with(suffix))
        .map(|f| f.contents.clone())
        .unwrap_or_else(|| panic!("no {suffix} among {:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>()))
}

#[test]
fn addresses_routers_and_targets_match_the_generators() {
    let spec = example("segmented");
    let r = resolve(&spec);
    assert_eq!(lookup(&r, "resolved_version").unwrap(), 1);
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
    // Targets: what generates, and why the others don't.
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
    // Checks by position, with the runner each target names them by.
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
