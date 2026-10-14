//! The ESXi module of a few examples, rendered from their plan, as snapshots; its determinism,
//! and its refusals read from the plan without rendering. Validated with `terraform validate`,
//! not run on a real host.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::esxi::EsxiBackend;

fn env(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::Esxi, None)
}

fn esxi(example: &str) -> String {
    EsxiBackend
        .render(&env(example))
        .expect("the backend renders the example")
        .into_iter()
        .find(|f| f.path.ends_with("esxi/main.tf"))
        .expect("the module")
        .contents
}

#[test]
fn esxi_hello_stack() {
    insta::assert_snapshot!(esxi("hello-stack"));
}

#[test]
fn esxi_segmented() {
    insta::assert_snapshot!(esxi("segmented"));
}

#[test]
fn esxi_ansible_pair() {
    insta::assert_snapshot!(esxi("ansible-pair"));
}

#[test]
fn the_module_is_the_same_each_time() {
    for example in ["hello-stack", "segmented", "ansible-pair"] {
        let e = env(example);
        assert_eq!(EsxiBackend.render(&e), EsxiBackend.render(&e), "{example}");
    }
}

#[test]
fn refusals_come_from_the_plan() {
    let windows = EsxiBackend.refusal(&env("windows-hello")).expect("refused");
    assert!(windows.contains("no ESXi image for `windows-server-2019`"), "{windows}");
    let v6 = EsxiBackend.refusal(&env("dual-stack")).expect("refused");
    assert!(v6.contains("IPv6"), "{v6}");
    let gateway = EsxiBackend.refusal(&env("edge-firewall")).expect("refused");
    assert!(gateway.contains("gateway"), "{gateway}");
    assert_eq!(EsxiBackend.refusal(&env("hello-stack")), None);
}
