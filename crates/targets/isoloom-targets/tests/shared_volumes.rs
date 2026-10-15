//! `volumes:` shared between machines: one Docker volume, one Kubernetes claim (its users on
//! the owner's node), and on VMs the owner's directory served over NFS to the others.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{Spec, Target, load, parse, validate};
use isoloom_targets::{generate, refusal};

fn shared() -> Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shared-files")).unwrap()
}

fn all(spec: &Spec, t: Target) -> String {
    generate(spec, t).unwrap().into_iter().map(|f| f.contents).collect()
}

#[test]
fn compose_mounts_one_volume_in_both() {
    let compose = generate(&shared(), Target::Docker)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("compose.yml"))
        .unwrap()
        .contents;
    assert!(compose.contains("- shared:/srv/shared") && compose.contains("- shared:/data/shared"));
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose).unwrap();
    let volumes: Vec<&str> = doc["volumes"].as_mapping().unwrap().keys().filter_map(|k| k.as_str()).collect();
    assert_eq!(
        volumes,
        ["shared", "writer-data"],
        "one volume, not one per machine (and the writer's data disk)"
    );
}

#[test]
fn kubernetes_has_one_claim_and_the_reader_beside_the_owner() {
    let k = all(&shared(), Target::Kubernetes);
    assert_eq!(
        k.matches("kind: PersistentVolumeClaim").count(),
        4,
        "plain manifests and the chart, one each, and the writer's data disk"
    );
    assert!(k.contains("claimName: shared"));
    assert!(k.contains("podAffinity") && k.contains("isoloom.com/machine: writer") && k.contains("topologyKey: kubernetes.io/hostname"));
}

#[test]
fn on_vms_the_owner_serves_it_and_the_reader_mounts_it_after() {
    let vf = generate(&shared(), Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents;
    let writer = vf.find("config.vm.define \"writer\"").unwrap();
    let reader = vf.find("config.vm.define \"reader\"").unwrap();
    assert!(writer < reader, "the owner starts first");
    assert!(vf.contains("nfs-kernel-server") && vf.contains("10.84.0.11(rw,sync,no_subtree_check,no_root_squash)"));
    assert!(vf.contains("10.84.0.10:/srv/shared /data/shared nfs defaults,_netdev 0 0"));
    assert!(all(&shared(), Target::CloudVm).contains("nfs-common"), "the clouds too");
}

#[test]
fn shared_volumes_are_validated() {
    let spec = parse(
        "version: 1
name: bad
networks: { a: { cidr: 10.84.0.0/24 }, b: { cidr: 10.85.0.0/24 } }
volumes: { lonely: {}, data: { owner: ghost }, apart: {} }
machines:
  one:
    networks: { a: 10 }
    volumes: { data: /d, apart: /x }
    docker: { image: alpine, idle: true }
  two:
    networks: { b: 10 }
    volumes: { apart: /y }
    docker: { image: alpine, idle: true }
",
    )
    .unwrap();
    let problems: Vec<String> = validate(&spec).into_iter().map(|p| p.to_string()).collect();
    let has = |at: &str, what: &str| problems.iter().any(|p| p.starts_with(at) && p.contains(what));
    assert!(has("volumes.lonely", "no machine mounts it"), "{problems:#?}");
    assert!(has("volumes.data.owner", "`ghost` doesn't mount it"), "{problems:#?}");
    assert!(has("machines.two.volumes.apart", "shares no network with `one`"), "{problems:#?}");
}

#[test]
fn hybrid_refuses_a_volume_shared_by_a_container_and_a_vm() {
    let mut spec = shared();
    spec.machines["reader"].docker = None;
    assert!(refusal(&spec, Target::Hybrid).unwrap().contains("shared by containers and VMs"));
}
