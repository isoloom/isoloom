use serde_json::{Value, json};

use crate::ManifestError;

/// The minimal fixture, changed by `patch`, then read: what `parse` says of it.
fn read(patch: impl FnOnce(&mut Value)) -> Result<(), ManifestError> {
    let mut doc: Value = serde_json::from_str(include_str!("../../fixtures/minimal.run.json")).unwrap();
    patch(&mut doc);
    crate::parse(&doc.to_string()).map(|_| ())
}

/// The code `parse` refuses the patched fixture with.
fn refused(patch: impl FnOnce(&mut Value)) -> &'static str {
    read(patch).unwrap_err().code()
}

/// An input slot delivered as `delivery`.
fn slot(delivery: &Value) -> Value {
    json!([{ "name": "FLAG", "kind": "input", "required": true, "sensitive": true, "deliver": [delivery] }])
}

/// A param delivered as `delivery`.
fn param(delivery: &Value) -> Value {
    json!([{ "name": "REGION", "description": "Where", "set_by": "embedder", "deliver": [delivery] }])
}

#[test]
fn the_minimal_fixture_is_valid() {
    assert_eq!(read(|_| {}), Ok(()));
}

#[test]
fn a_manifest_runs_from_some_host() {
    assert_eq!(refused(|d| d["hosts"] = json!([])), "no_hosts");
}

#[test]
fn the_identity_is_the_drivers() {
    assert_eq!(refused(|d| d["driver"] = json!("vagrant")), "driver_identity");
}

#[test]
fn a_guest_delivery_names_a_machine_of_the_manifest() {
    let guest = |machine: &str| json!({ "via": "guest", "machine": machine, "transport": "ssh" });
    assert_eq!(read(|d| d["secrets"] = slot(&guest("web"))), Ok(()));
    let err = read(|d| d["secrets"] = slot(&guest("db"))).unwrap_err();
    assert!(
        matches!(&err, ManifestError::UnknownMachine { at, machine } if at == "secrets[0].deliver[0]" && machine == "db"),
        "{err}"
    );
}

#[test]
fn a_guest_delivery_during_a_step_names_a_machine_and_a_step() {
    let during = |machine: &str, step: &str| json!({ "via": "guest", "machine": machine, "transport": "ssh", "during_step": step });
    assert_eq!(read(|d| d["secrets"] = slot(&during("web", "up"))), Ok(()));
    assert_eq!(refused(|d| d["secrets"] = slot(&during("db", "up"))), "unknown_machine");
    let err = read(|d| d["secrets"] = slot(&during("web", "boot"))).unwrap_err();
    assert!(matches!(&err, ManifestError::UnknownStep { step, .. } if step == "boot"), "{err}");
}

#[test]
fn an_environment_delivery_names_steps_of_the_manifest() {
    let env = |steps: Value| json!({ "via": "env", "var": "REGION", "steps": steps });
    assert_eq!(read(|d| d["params"] = param(&env(json!(["up", "down"])))), Ok(()));
    assert_eq!(read(|d| d["params"] = param(&env(json!(["*"])))), Ok(()));
    let err = read(|d| d["params"] = param(&env(json!(["up", "boot"])))).unwrap_err();
    assert!(
        matches!(&err, ManifestError::UnknownStep { at, step } if at == "params[0].deliver[0]" && step == "boot"),
        "{err}"
    );
}

#[test]
fn a_ready_condition_names_an_up_step_and_known_machines() {
    let ready = |c: Value| move |d: &mut Value| d["ready"] = json!([c]);
    assert_eq!(read(ready(json!({ "kind": "machines_running", "machines": ["web"] }))), Ok(()));
    let err = read(ready(json!({ "kind": "machines_running", "machines": ["web", "db"] }))).unwrap_err();
    assert!(matches!(&err, ManifestError::UnknownMachine { at, .. } if at == "ready[0]"), "{err}");
    assert_eq!(refused(ready(json!({ "kind": "step_ok", "step": "down" }))), "ready_not_up");
}

#[test]
fn machines_and_their_ports_are_named_once() {
    assert_eq!(
        refused(|d| {
            let web = d["machines"][0].clone();
            d["machines"].as_array_mut().unwrap().push(web);
        }),
        "duplicate_machine"
    );
    assert_eq!(
        refused(|d| {
            let site = d["machines"][0]["ports"][0].clone();
            d["machines"][0]["ports"].as_array_mut().unwrap().push(site);
        }),
        "duplicate_port"
    );
}

#[test]
fn a_machine_reference_names_a_machine() {
    assert_eq!(refused(|d| d["machines"][0]["of"] = json!("db")), "unknown_machine");
    assert_eq!(read(|d| d["machines"][0]["of"] = json!("web")), Ok(()));
    let mut with_machines = slot(&json!({ "via": "env", "var": "FLAG", "steps": ["up"] }));
    with_machines[0]["machines"] = json!(["db"]);
    let err = read(|d| d["secrets"] = with_machines).unwrap_err();
    assert!(matches!(&err, ManifestError::UnknownMachine { at, .. } if at == "secrets[0].machines"), "{err}");
}

#[test]
fn an_output_reads_a_published_port() {
    assert_eq!(refused(|d| d["outputs"][0]["from"]["port"] = json!("admin")), "unknown_published");
    // A declared port that isn't published has no host port to read.
    let err = read(|d| {
        d["machines"][0]["ports"][0].as_object_mut().unwrap().remove("publish");
    })
    .unwrap_err();
    assert!(matches!(&err, ManifestError::UnknownPublished { port, .. } if port == "web.site"), "{err}");
}

#[test]
fn a_param_or_output_placeholder_names_a_declared_one() {
    let message = |text: &str| {
        let text = text.to_owned();
        move |d: &mut Value| d["lab"]["message"] = json!(text)
    };
    assert_eq!(refused(message("${params.REGION}")), "unknown_param");
    assert_eq!(
        read(|d| {
            d["params"] = param(&json!({ "via": "env", "var": "REGION", "steps": ["up"] }));
            d["lab"]["message"] = json!("in ${params.REGION}");
        }),
        Ok(())
    );
    assert_eq!(refused(message("${outputs.admin}")), "unknown_output");
    assert_eq!(read(message("at ${outputs.web_site}")), Ok(()));
}

#[test]
fn value_and_machine_are_bound_only_where_they_are_given() {
    assert_eq!(refused(|d| d["lab"]["message"] = json!("${value}")), "placeholder_out_of_place");
    assert_eq!(refused(|d| d["lab"]["message"] = json!("${machine}")), "placeholder_out_of_place");
    let provision = json!({ "machine": [{ "id": "again", "argv": ["docker", "compose", "restart", "${machine}"] }] });
    assert_eq!(read(|d| d["lifecycle"]["provision"] = provision), Ok(()));
    let all = json!({ "all": [{ "id": "again", "argv": ["docker", "compose", "restart", "${machine}"] }] });
    let err = read(|d| d["lifecycle"]["provision"] = all).unwrap_err();
    assert!(matches!(&err, ManifestError::PlaceholderOutOfPlace { root, .. } if *root == "machine"), "{err}");
}

/// A message naming an undeclared param, placed in the template at `place` instead.
fn unknown_param_at(place: impl FnOnce(&mut Value)) -> String {
    match read(place) {
        Err(ManifestError::UnknownParam { at, .. }) => at,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_template_of_a_manifest_is_checked_where_it_sits() {
    let undeclared = json!("${params.NOPE}");
    assert_eq!(
        unknown_param_at(|d| d["machines"][0]["ports"][0]["publish"]["address"] = undeclared.clone()),
        "machines[0].ports[0].publish.address"
    );
    assert_eq!(
        unknown_param_at(|d| d["lifecycle"]["up"][0]["argv"][3] = undeclared.clone()),
        "lifecycle.up[0].argv[3]"
    );
    assert_eq!(
        unknown_param_at(|d| d["lifecycle"]["up"][0]["argv"] = json!({ "default": ["docker", "${params.NOPE}"], "windows": ["docker", "x"] })),
        "lifecycle.up[0].argv.default[1]"
    );
    assert_eq!(
        unknown_param_at(|d| d["lifecycle"]["up"][0]["env"] = json!({ "TF_DATA_DIR": "${params.NOPE}" })),
        "lifecycle.up[0].env.TF_DATA_DIR"
    );
    assert_eq!(unknown_param_at(|d| d["networks"][0]["native"] = undeclared.clone()), "networks[0].native");
    assert_eq!(
        unknown_param_at(|d| d["tools"][0]["probe"] = json!(["docker", "${params.NOPE}"])),
        "tools[0].probe[1]"
    );
    let ready = json!({ "kind": "tcp", "address": "127.0.0.1", "port": "${params.NOPE}", "timeout_s": 5 });
    assert_eq!(unknown_param_at(|d| d["ready"] = json!([ready])), "ready[0].port");
    let file = json!({ "kind": "ready_file", "host": "h", "user": "u", "path": "/ready", "via": "${params.NOPE}", "grammar": "isoloom-ready/1", "timeout_s": 5, "interval_s": 1 });
    assert_eq!(unknown_param_at(|d| d["ready"] = json!([file])), "ready[0].via");
    let action = json!({ "id": "pin", "action": "compose.pin_ports", "with": { "file": "${params.NOPE}" } });
    assert_eq!(
        unknown_param_at(|d| d["lifecycle"]["up"] = json!([d["lifecycle"]["up"][0].clone(), action])),
        "lifecycle.up[1].with.file"
    );
}
