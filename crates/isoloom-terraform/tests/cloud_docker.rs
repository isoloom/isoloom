//! The cloud-docker modules of hello-stack on every cloud, docker-vm's Proxmox module, and the
//! cloud-services scripts of cloud-bucket, rendered from their plan, as snapshots; and their
//! determinism.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::fmt::Write as _;

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::{Backend, Files};
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_docker::CloudDockerBackend;
use isoloom_terraform::cloud_services::CloudServicesBackend;

fn env(example: &str, target: Target) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, target, None)
}

fn file(files: &Files, suffix: &str) -> String {
    files.iter().find(|f| f.path.ends_with(suffix)).expect("the file").contents.clone()
}

fn cloud(cloud: &str) -> String {
    let files = CloudDockerBackend.render(&env("hello-stack", Target::CloudDocker)).expect("renders");
    file(&files, &format!("cloud-docker/{cloud}/main.tf"))
}

#[test]
fn cloud_docker_aws_hello_stack() {
    insta::assert_snapshot!(cloud("aws"));
}

#[test]
fn cloud_docker_azure_hello_stack() {
    insta::assert_snapshot!(cloud("azure"));
}

#[test]
fn cloud_docker_gcp_hello_stack() {
    insta::assert_snapshot!(cloud("gcp"));
}

#[test]
fn cloud_docker_digitalocean_hello_stack() {
    insta::assert_snapshot!(cloud("digitalocean"));
}

#[test]
fn cloud_docker_linode_hello_stack() {
    insta::assert_snapshot!(cloud("linode"));
}

#[test]
fn cloud_docker_oci_hello_stack() {
    insta::assert_snapshot!(cloud("oci"));
}

#[test]
fn docker_vm_proxmox_hello_stack() {
    let tf = isoloom_terraform::cloud_docker::proxmox::render(&env("hello-stack", Target::DockerVm)).unwrap();
    insta::assert_snapshot!(tf.contents);
}

#[test]
fn cloud_services_cloud_bucket() {
    let files = CloudServicesBackend.render(&env("cloud-bucket", Target::CloudServices)).expect("renders");
    let all = files.iter().fold(String::new(), |mut s, f| {
        let _ = write!(s, "==> {}\n{}", f.path, f.contents);
        s
    });
    insta::assert_snapshot!(all);
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "udp-dns", "segmented", "workbench"] {
        let e = env(example, Target::CloudDocker);
        assert_eq!(CloudDockerBackend.render(&e), CloudDockerBackend.render(&e), "{example}");
    }
    let e = env("cloud-bucket", Target::CloudServices);
    assert_eq!(CloudServicesBackend.render(&e), CloudServicesBackend.render(&e));
}

#[test]
fn a_machine_needing_a_controller_is_refused_by_capabilities() {
    let e = env("ansible-pair", Target::CloudDocker);
    let why = CloudDockerBackend.refusal(&e).expect("refused");
    assert!(why.contains("provision"), "{why}");
    assert!(CloudDockerBackend.render(&e).is_err());
}
