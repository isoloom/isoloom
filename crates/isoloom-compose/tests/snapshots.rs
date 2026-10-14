//! The Compose backend's rendering of a few examples, as snapshots, and its determinism.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The files the Compose backend renders for an example, from its plan.
fn render(example: &str) -> Vec<isoloom_spec::files::GeneratedFile> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    let env = Environment::build(&spec, Target::Docker, None);
    isoloom_compose::render(Target::Docker, &env).expect("Compose renders the example")
}

fn compose(example: &str) -> String {
    render(example)
        .into_iter()
        .find(|f| f.path.ends_with("compose.yml"))
        .expect("a Compose file")
        .contents
}

#[test]
fn hello_stack() {
    insta::assert_snapshot!(compose("hello-stack"));
}

#[test]
fn segmented_routes_through_the_router() {
    insta::assert_snapshot!(compose("segmented"));
}

#[test]
fn shared_files_share_one_volume() {
    insta::assert_snapshot!(compose("shared-files"));
}

#[test]
fn tuned_web_settings() {
    insta::assert_snapshot!(compose("tuned-web"));
}

#[test]
fn edge_firewall_gateway_machine() {
    insta::assert_snapshot!(compose("edge-firewall"));
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "segmented", "vlan-office", "cisco-iol"] {
        assert_eq!(render(example), render(example), "{example}");
    }
}

#[test]
fn provisioning_is_refused_without_rendering() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    docker: { image: alpine }\n    vm: { os: debian-12 }\nprovision:\n  - ansible: site.yml\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::Docker, None);
    let err = isoloom_compose::render(Target::Docker, &env).expect_err("refused");
    assert_eq!(
        err.to_string(),
        "docker: environment-level provisioning (`provision:`) runs on VM targets for now"
    );
}
