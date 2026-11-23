//! isoloom-wasm's one test binary: the functions the editor calls, as JavaScript sees them. It
//! runs in WebAssembly only, with `wasm-pack test --node crates/frontends/isoloom-wasm`
//! (`cargo xtask check --only wasm-test`); natively it holds no test.
#![cfg(target_arch = "wasm32")]

use isoloom_wasm::{bundle, bundle_target, choices_json, own_files_json, plan_json};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

/// Two machines on a cable: a VM with a step, and a web server.
const TOPOLOGY: &str = include_str!("../fixtures/pair.json");

const MALFORMED: &str = "{ not json";

/// The message of a `JsError` as JavaScript reads it (`error.message`).
fn message(e: wasm_bindgen::JsError) -> String {
    let value = JsValue::from(e);
    let error: &js_error::Error = wasm_bindgen::JsCast::unchecked_ref(&value);
    error.message()
}

/// JavaScript's `Error`, for its `message`.
mod js_error {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub type Error;
        #[wasm_bindgen(method, getter)]
        pub fn message(this: &Error) -> String;
    }
}

#[wasm_bindgen_test]
fn plan_json_is_a_plan() {
    let plan: serde_json::Value = serde_json::from_str(&plan_json(TOPOLOGY).map_err(message).unwrap()).unwrap();
    assert_eq!(plan["ok"], true);
    let spec: serde_yaml_ng::Value = serde_yaml_ng::from_str(plan["isoloom_yml"].as_str().unwrap()).unwrap();
    assert_eq!(spec["name"].as_str(), Some("pair"));
    let bad: serde_json::Value = serde_json::from_str(&plan_json(MALFORMED).map_err(message).unwrap()).unwrap();
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().starts_with("the topology is malformed: "), "{bad}");
}

#[wasm_bindgen_test]
fn own_files_json_lists_the_starters() {
    let files: serde_json::Value = serde_json::from_str(&own_files_json(TOPOLOGY).map_err(message).unwrap()).unwrap();
    assert_eq!(files[0]["path"], "provision/client.sh");
    let refused = message(own_files_json(MALFORMED).unwrap_err());
    assert!(refused.starts_with("the topology is malformed: "), "{refused}");
}

#[wasm_bindgen_test]
fn bundle_is_a_zip() {
    let zip = bundle(TOPOLOGY).unwrap();
    assert_eq!(&zip[..4], b"PK\x03\x04");
    let refused = message(bundle(MALFORMED).unwrap_err());
    assert!(refused.starts_with("the topology is malformed: "), "{refused}");
}

#[wasm_bindgen_test]
fn bundle_target_refuses_a_target_the_project_has_not() {
    assert_eq!(&bundle_target(TOPOLOGY, "docker").unwrap()[..2], b"PK");
    assert_eq!(
        message(bundle_target(TOPOLOGY, "nowhere").unwrap_err()),
        "this project has no files for `nowhere`"
    );
}

#[wasm_bindgen_test]
fn choices_json_lists_isoloom_s_values() {
    let c: serde_json::Value = serde_json::from_str(&choices_json().map_err(message).unwrap()).unwrap();
    assert!(c["os"].as_array().unwrap().iter().any(|os| os == "debian@12"));
    assert_eq!(c["protocols"], serde_json::json!(["tcp", "udp", "http", "https", "tls"]));
}
