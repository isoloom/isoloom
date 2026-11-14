//! The functions the editor calls, as JavaScript sees them: run in WebAssembly with
//! `wasm-pack test --node crates/frontends/isoloom-wasm` (CI does).
#![cfg(target_arch = "wasm32")]

use isoloom_wasm::{bundle, bundle_target, choices_json, own_files_json, plan_json};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

/// Two machines on a cable: a VM with a step, and a web server.
const TOPOLOGY: &str = include_str!("fixtures/pair.json");

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
    assert!(plan["isoloom_yml"].as_str().unwrap().contains("name: pair"));
    let bad: serde_json::Value = serde_json::from_str(&plan_json(MALFORMED).map_err(message).unwrap()).unwrap();
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().contains("malformed"));
}

#[wasm_bindgen_test]
fn own_files_json_lists_the_starters() {
    let files: serde_json::Value = serde_json::from_str(&own_files_json(TOPOLOGY).map_err(message).unwrap()).unwrap();
    assert_eq!(files[0]["path"], "provision/client.sh");
    assert!(message(own_files_json(MALFORMED).unwrap_err()).contains("malformed"));
}

#[wasm_bindgen_test]
fn bundle_is_a_zip() {
    let zip = bundle(TOPOLOGY).unwrap();
    assert_eq!(&zip[..4], b"PK\x03\x04");
    assert!(message(bundle(MALFORMED).unwrap_err()).contains("malformed"));
}

#[wasm_bindgen_test]
fn bundle_target_refuses_a_target_the_project_has_not() {
    assert_eq!(&bundle_target(TOPOLOGY, "docker").unwrap()[..2], b"PK");
    let e = message(bundle_target(TOPOLOGY, "nowhere").unwrap_err());
    assert!(e.contains("no files for `nowhere`"), "{e}");
}

#[wasm_bindgen_test]
fn choices_json_lists_isoloom_s_values() {
    let c: serde_json::Value = serde_json::from_str(&choices_json().map_err(message).unwrap()).unwrap();
    assert!(c["os"].as_array().unwrap().iter().any(|os| os == "debian@12"));
    assert_eq!(c["protocols"][0], "tcp");
}
