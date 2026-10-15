//! The `cloud-docker` target put together: the `docker` target's Compose file with a module per
//! cloud, and its refusals, Compose's capabilities first, then the clouds' own.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_targets::composite::CloudDockerBackend;

fn env(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::CloudDocker, None)
}

#[test]
fn the_compose_file_comes_first_then_a_module_per_cloud() {
    let files = CloudDockerBackend.render(&env("hello-stack")).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let compose = paths.iter().position(|p| *p == ".isoloom/docker/compose.yml").expect("the Compose file");
    let first_module = paths.iter().position(|p| p.starts_with(".isoloom/cloud-docker/")).expect("a module");
    assert!(compose < first_module, "{paths:?}");
    for cloud in isoloom_terraform::cloud_docker::Cloud::ALL {
        assert!(paths.contains(&format!(".isoloom/cloud-docker/{}/main.tf", cloud.id()).as_str()), "{paths:?}");
    }
}

#[test]
fn a_machine_needing_a_controller_is_refused_by_capabilities() {
    let e = env("ansible-pair");
    let why = CloudDockerBackend.refusal(&e).expect("refused");
    assert!(why.contains("provision"), "{why}");
    assert!(CloudDockerBackend.render(&e).is_err());
}

#[test]
fn arm64_is_refused_by_the_clouds() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    arch: arm64\n    docker: { image: nginx }\n",
    )
    .unwrap();
    let e = Environment::build(&spec, Target::CloudDocker, None);
    let why = CloudDockerBackend.refusal(&e).unwrap();
    assert_eq!(why, "machine `w`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)");
    assert!(CloudDockerBackend.render(&e).unwrap_err().to_string().ends_with(&why));
}
