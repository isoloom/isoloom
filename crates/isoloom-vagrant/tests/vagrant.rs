//! The Vagrant backend's rendering of a few examples, as snapshots, its determinism, and its
//! refusals from the plan.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_vagrant::vagrant::{self, VagrantBackend};

fn env_of(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::Vagrant, None)
}

/// The files the Vagrant backend renders for an example, from its plan.
fn render(example: &str) -> Vec<isoloom_spec::files::GeneratedFile> {
    vagrant::render(&env_of(example)).expect("Vagrant renders the example")
}

fn vagrantfile(example: &str) -> String {
    render(example)
        .into_iter()
        .find(|f| f.path.ends_with("/Vagrantfile"))
        .expect("a Vagrantfile")
        .contents
}

#[test]
fn hello_stack() {
    insta::assert_snapshot!(vagrantfile("hello-stack"));
}

#[test]
fn segmented_routes_through_the_router() {
    insta::assert_snapshot!(vagrantfile("segmented"));
}

#[test]
fn windows_hello_over_winrm() {
    insta::assert_snapshot!(vagrantfile("windows-hello"));
}

#[test]
fn tuned_web_settings() {
    insta::assert_snapshot!(vagrantfile("tuned-web"));
}

#[test]
fn shared_files_over_nfs() {
    insta::assert_snapshot!(vagrantfile("shared-files"));
}

#[test]
fn ansible_pair_from_the_controller() {
    insta::assert_snapshot!(vagrantfile("ansible-pair"));
}

#[test]
fn rendering_twice_gives_the_same_files() {
    for example in ["hello-stack", "segmented", "edge-firewall", "vlan-office", "existing-hosts"] {
        assert_eq!(render(example), render(example), "{example}");
    }
}

#[test]
fn the_registry_renders_vagrant_from_the_plan() {
    let env = env_of("hello-stack");
    assert_eq!(VagrantBackend.render(&env).unwrap(), render("hello-stack"));
    assert_eq!(VagrantBackend.refusal(&env), None);
}

#[test]
fn appliances_are_refused_from_the_capabilities() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  sw:\n    networks: { lab: 10 }\n    docker: { image: x, appliance: cisco-iol }\n    vm: { os: debian@12 }\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::Vagrant, None);
    let why = VagrantBackend.refusal(&env).expect("refused");
    assert!(why.contains("network appliance"), "{why}");
    assert!(vagrant::render(&env).is_err());
}

#[test]
fn a_machine_without_a_box_is_refused_without_rendering() {
    let mut spec = isoloom_spec::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  m:\n    networks: { lab: 10 }\n    vm: { os: debian@12 }\n",
    )
    .expect("parses");
    // Parsing refuses an OS it doesn't know; a spec built in code can still name one.
    spec.machines["m"].vm.as_mut().expect("a VM").os = "plan9".into();
    let env = Environment::build(&spec, Target::Vagrant, None);
    let why = VagrantBackend.refusal(&env).expect("refused");
    assert!(why.contains("no Vagrant box for `plan9`"), "{why}");
    assert_eq!(vagrant::render(&env).unwrap_err().to_string(), format!("vagrant: {why}"));
}
