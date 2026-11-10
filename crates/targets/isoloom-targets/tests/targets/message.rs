//! `message:`: shown after `run`, its placeholders filled from the resolved snapshot.

use isoloom_spec::parse;
use isoloom_targets::Lab;
use isoloom_targets::resolved::{fill, render_message, resolve};

use crate::testing::{Outcome, example};

/// The hello-stack example.
fn hello_stack() -> Outcome<Lab> {
    example("hello-stack")
}

#[test]
fn placeholders_take_any_value_of_the_snapshot() -> Outcome {
    let spec = hello_stack()?;
    let m = render_message(&spec, None).unwrap().unwrap();
    assert!(m.contains("http://127.0.0.1:8080/"), "{m}");
    assert!(m.contains("10.60.0.20:6379"), "{m}");
    // As an instance, the published port follows.
    assert!(render_message(&spec, Some(2)).unwrap().unwrap().contains("http://127.0.0.1:8280/"));
    // The snapshot carries the rendered text.
    assert_eq!(resolve(&spec)["message"].as_str().map(|s| s.contains("8080")), Some(true));
    Ok(())
}

#[test]
fn on_docker_the_message_gives_the_addresses_machines_have_there() -> Outcome {
    use isoloom_spec::Target;
    use isoloom_targets::resolved::render_message_on;
    let spec = hello_stack()?;
    // Instance 1 runs on Docker at 10.61.0.0/24; its VMs keep the spec's 10.60.0.0/24.
    let docker = render_message_on(&spec, Some(1), Target::Docker).unwrap().unwrap();
    assert!(docker.contains("10.61.0.20:6379") && docker.contains("127.0.0.1:8180"), "{docker}");
    let vagrant = render_message_on(&spec, Some(1), Target::Vagrant).unwrap().unwrap();
    assert!(vagrant.contains("10.60.0.20:6379"), "{vagrant}");
    Ok(())
}

#[test]
fn a_placeholder_pointing_at_nothing_is_an_error_and_syntax_is_validated() {
    let snapshot = serde_json::json!({ "machines": { "a": { "addresses": { "lab": "10.9.0.5", "both": [1, 2] }, "ports": { "p": { "port": 3 } } } } });
    assert_eq!(
        fill(
            "{{ machines.a.addresses.lab }}-{{machines.a.ports.p.port}} {{ machines.a.addresses.both }}",
            &snapshot
        )
        .unwrap(),
        "10.9.0.5-3 [1,2]"
    );
    assert!(
        fill("{{ machines.b.addresses.lab }}", &snapshot)
            .unwrap_err()
            .to_string()
            .contains("points at nothing")
    );
    // One grammar for every placeholder: a path the message can't have is said so.
    let grammar = fill("{{ a.nope }}", &snapshot).unwrap_err().to_string();
    assert!(grammar.contains("the paths start with `machine.`"), "{grammar}");
    assert!(fill("{{ machines.a.addresses.lab", &snapshot).unwrap_err().to_string().contains("without its"));
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x } }\n";
    let bad = Lab::new(parse(&format!("{base}message: \"see {{{{ }}}} and {{{{ a..b }}}} and {{{{ open\"\n")).unwrap());
    let problems: Vec<String> = bad.problems().iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().all(|p| p.starts_with("message:")), "{problems:?}");
}

#[test]
fn a_cloud_output_is_the_runners_in_the_manifest_and_stays_as_written_until_then() -> Outcome {
    use isoloom_spec::Target;
    let lab = example("cloud-bucket")?;
    let files = isoloom_targets::generate(&lab, Target::CloudServices)?;
    let manifest = isoloom_manifest::parse(crate::testing::file(&files, "cloud-services/run.json")?)?;
    assert_eq!(
        manifest.environment.message.map(|m| m.to_string()).as_deref(),
        Some("The website answers at ${outputs.site}.\n")
    );
    let shown = render_message(&lab, None)?.unwrap_or_default();
    assert_eq!(shown, "The website answers at {{ cloud.outputs.site }}.\n");
    assert_eq!(resolve(&lab)["message"].as_str(), Some(shown.as_str()));
    Ok(())
}
