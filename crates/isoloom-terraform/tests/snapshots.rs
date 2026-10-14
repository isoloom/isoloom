//! The AWS, Azure, Google Cloud and Proxmox modules of a few examples, rendered from their plan, as snapshots; their
//! determinism, and refusals read from the plan without rendering.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::CloudVmBackend;
use isoloom_terraform::proxmox::ProxmoxBackend;

fn env(example: &str, target: Target) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, target, None)
}

/// The module at `suffix` that `backend` renders for `example`.
fn module(backend: &dyn Backend, example: &str, suffix: &str) -> String {
    backend
        .render(&env(example, backend.target()))
        .expect("the backend renders the example")
        .into_iter()
        .find(|f| f.path.ends_with(suffix))
        .expect("the module")
        .contents
}

fn aws(example: &str) -> String {
    module(&CloudVmBackend, example, "cloud-vm/aws/main.tf")
}

fn azure(example: &str) -> String {
    module(&CloudVmBackend, example, "cloud-vm/azure/main.tf")
}

fn gcp(example: &str) -> String {
    module(&CloudVmBackend, example, "cloud-vm/gcp/main.tf")
}

fn proxmox(example: &str) -> String {
    module(&ProxmoxBackend, example, "proxmox/main.tf")
}

#[test]
fn aws_hello_stack() {
    insta::assert_snapshot!(aws("hello-stack"));
}

#[test]
fn aws_segmented() {
    insta::assert_snapshot!(aws("segmented"));
}

#[test]
fn aws_windows_hello() {
    insta::assert_snapshot!(aws("windows-hello"));
}

#[test]
fn aws_ansible_pair() {
    insta::assert_snapshot!(aws("ansible-pair"));
}

#[test]
fn azure_hello_stack() {
    insta::assert_snapshot!(azure("hello-stack"));
}

#[test]
fn azure_segmented() {
    insta::assert_snapshot!(azure("segmented"));
}

#[test]
fn azure_windows_hello() {
    insta::assert_snapshot!(azure("windows-hello"));
}

#[test]
fn gcp_solo_web() {
    insta::assert_snapshot!(gcp("solo-web"));
}

#[test]
fn gcp_hello_stack() {
    insta::assert_snapshot!(gcp("hello-stack"));
}

#[test]
fn proxmox_hello_stack() {
    insta::assert_snapshot!(proxmox("hello-stack"));
}

#[test]
fn proxmox_segmented() {
    insta::assert_snapshot!(proxmox("segmented"));
}

#[test]
fn proxmox_ansible_pair() {
    insta::assert_snapshot!(proxmox("ansible-pair"));
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "segmented", "vlan-office", "existing-hosts", "windows-hello"] {
        let e = env(example, Target::CloudVm);
        assert_eq!(CloudVmBackend.render(&e), CloudVmBackend.render(&e), "{example}");
    }
    for example in ["hello-stack", "ansible-pair", "existing-hosts"] {
        let e = env(example, Target::Proxmox);
        assert_eq!(ProxmoxBackend.render(&e), ProxmoxBackend.render(&e), "{example}");
    }
}

#[test]
fn refusals_come_from_the_plan_and_match_rendering() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    arch: arm64\n    vm: { os: debian-12 }\n",
    )
    .unwrap();
    for (backend, why) in [
        (
            &CloudVmBackend as &dyn Backend,
            "machine `w`: arm64 on AWS comes later (Graviton instances and arm64 AMIs)",
        ),
        (
            &ProxmoxBackend,
            "machine `w`: arm64 on Proxmox comes later (arm64 is uncommon on Proxmox hosts)",
        ),
    ] {
        let e = Environment::build(&spec, backend.target(), None);
        assert_eq!(backend.refusal(&e).as_deref(), Some(why));
        let err = backend.render(&e).expect_err("refused");
        assert!(err.to_string().ends_with(why), "{err}");
    }
}

#[test]
fn a_windows_machine_is_refused_by_proxmox_only() {
    let e = |t| env("windows-hello", t);
    assert!(CloudVmBackend.refusal(&e(Target::CloudVm)).is_none());
    let why = ProxmoxBackend.refusal(&e(Target::Proxmox)).expect("refused");
    assert!(why.contains("no Proxmox image for `windows-server-2019`"), "{why}");
}

#[test]
fn each_cloud_refuses_on_its_own() {
    use isoloom_terraform::cloud_vm::cloud_refusals;
    // Two networks, each machine on one: AWS, Azure and Google Cloud take it.
    let refusals = cloud_refusals(&env("segmented", Target::CloudVm));
    let names: Vec<&str> = refusals.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["aws", "azure", "gcp", "digitalocean", "linode", "oci"]);
    assert!(refusals[..3].iter().all(|(_, why)| why.is_none()), "{refusals:?}");
    // A Windows machine: Google Cloud only.
    let refusals = cloud_refusals(&env("windows-hello", Target::CloudVm));
    assert_eq!(refusals[2].1.as_deref(), Some("machine `web01`: Windows on GCP comes later"));
    let files = CloudVmBackend.render(&env("windows-hello", Target::CloudVm)).unwrap();
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/azure/main.tf")));
    assert!(!files.iter().any(|f| f.path.ends_with("cloud-vm/gcp/main.tf")));
}

#[test]
fn a_cloud_refuses_from_the_plan_without_rendering() {
    use isoloom_terraform::cloud_vm::CLOUDS;
    let spec = isoloom_spec::parse(
        "version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    arch: arm64\n    vm: { os: debian-12 }\n",
    )
    .unwrap();
    let e = Environment::build(&spec, Target::CloudVm, None);
    for (name, why) in [
        ("azure", "machine `w`: arm64 on Azure comes later (Arm-based sizes and arm64 images)"),
        (
            "gcp",
            "machine `w`: arm64 on Google Cloud comes later (Arm-based machine types and arm64 images)",
        ),
    ] {
        let (_, cloud) = CLOUDS.iter().find(|(n, _)| *n == name).unwrap();
        assert_eq!(cloud.refusal(&e).as_deref(), Some(why));
        assert!(cloud.render(&e).unwrap_err().to_string().ends_with(why));
    }
}
