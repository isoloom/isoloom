//! `message:`: shown after `run`, its placeholders filled from the resolved snapshot.

use std::path::Path;

use isoloom_core::resolved::{fill, render_message, resolve};
use isoloom_core::{load, parse, validate};

#[test]
fn placeholders_take_any_value_of_the_snapshot() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-stack")).unwrap();
    let m = render_message(&spec, None).unwrap().unwrap();
    assert!(m.contains("http://localhost:8080/"), "{m}");
    assert!(m.contains("10.60.0.20:6379"), "{m}");
    // As an instance, the published port follows.
    let two = isoloom_core::instance::apply(&spec, 2).unwrap();
    assert!(render_message(&two, Some(2)).unwrap().unwrap().contains("http://localhost:8280/"));
    // The snapshot carries the rendered text.
    assert_eq!(resolve(&spec)["message"].as_str().map(|s| s.contains("8080")), Some(true));
}

#[test]
fn a_placeholder_pointing_at_nothing_is_an_error_and_syntax_is_validated() {
    let snapshot = serde_json::json!({ "a": { "b": "x", "n": 3, "list": [1, 2] } });
    assert_eq!(fill("{{ a.b }}-{{a.n}} {{ a.list }}", &snapshot).unwrap(), "x-3 [1,2]");
    assert!(fill("{{ a.nope }}", &snapshot).unwrap_err().contains("points at nothing"));
    assert!(fill("{{ a.b", &snapshot).unwrap_err().contains("without its"));
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, docker: { image: x } }\n";
    let bad = parse(&format!("{base}message: \"see {{{{ }}}} and {{{{ a..b }}}} and {{{{ open\"\n")).unwrap();
    let problems: Vec<String> = validate(&bad).into_iter().map(|p| p.to_string()).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().all(|p| p.starts_with("message:")), "{problems:?}");
}
