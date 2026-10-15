//! `message:`: shown after `run`, its placeholders filled from the resolved snapshot.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{load, parse};
use isoloom_targets::Lab;
use isoloom_targets::resolved::{fill, render_message, resolve};

fn hello_stack() -> Lab {
    Lab::new(load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/hello-stack")).unwrap())
}

#[test]
fn placeholders_take_any_value_of_the_snapshot() {
    let spec = hello_stack();
    let m = render_message(&spec, None).unwrap().unwrap();
    assert!(m.contains("http://127.0.0.1:8080/"), "{m}");
    assert!(m.contains("10.60.0.20:6379"), "{m}");
    // As an instance, the published port follows.
    assert!(render_message(&spec, Some(2)).unwrap().unwrap().contains("http://127.0.0.1:8280/"));
    // The snapshot carries the rendered text.
    assert_eq!(resolve(&spec)["message"].as_str().map(|s| s.contains("8080")), Some(true));
}

#[test]
fn on_docker_the_message_gives_the_addresses_machines_have_there() {
    use isoloom_spec::Target;
    use isoloom_targets::resolved::render_message_on;
    let spec = hello_stack();
    // Instance 1 runs on Docker at 10.61.0.0/24; its VMs keep the spec's 10.60.0.0/24.
    let docker = render_message_on(&spec, Some(1), Target::Docker).unwrap().unwrap();
    assert!(docker.contains("10.61.0.20:6379") && docker.contains("127.0.0.1:8180"), "{docker}");
    let vagrant = render_message_on(&spec, Some(1), Target::Vagrant).unwrap().unwrap();
    assert!(vagrant.contains("10.60.0.20:6379"), "{vagrant}");
}

#[test]
fn a_placeholder_pointing_at_nothing_is_an_error_and_syntax_is_validated() {
    let snapshot = serde_json::json!({ "a": { "b": "x", "n": 3, "list": [1, 2] } });
    assert_eq!(fill("{{ a.b }}-{{a.n}} {{ a.list }}", &snapshot).unwrap(), "x-3 [1,2]");
    assert!(fill("{{ a.nope }}", &snapshot).unwrap_err().contains("points at nothing"));
    assert!(fill("{{ a.b", &snapshot).unwrap_err().contains("without its"));
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x } }\n";
    let bad = Lab::new(parse(&format!("{base}message: \"see {{{{ }}}} and {{{{ a..b }}}} and {{{{ open\"\n")).unwrap());
    let problems: Vec<String> = bad.problems().iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().all(|p| p.starts_with("message:")), "{problems:?}");
}

#[test]
fn the_message_says_the_host_port_the_environment_really_got() {
    use isoloom_spec::Target;
    use isoloom_targets::resolved::render_message_at;
    let m = render_message_at(&hello_stack(), None, Target::Docker, &[("web".into(), "web".into(), 32772)])
        .unwrap()
        .unwrap();
    assert!(m.contains("http://127.0.0.1:32772/"), "{m}");
}
