//! The functions JavaScript calls (`#[wasm_bindgen]`): the engine's values as JSON, and its errors
//! as JavaScript `Error`s whose `message` is the error's. A JavaScript `Error` exists only in
//! WebAssembly: built for another target (its tests, its documentation), an export that fails
//! panics.

use wasm_bindgen::prelude::*;

use crate::{bundle_bytes, bundle_target_bytes, choices, own_files, plan};

/// One target's zip: `isoloom.yml` and only that target's folder (`docker`, `hybrid`,
/// `cloud-docker/aws`...), with every one of the project's own files.
///
/// # Errors
///
/// A JavaScript `Error` saying why: as [`bundle`], or the project generates nothing for
/// `target_dir`.
#[wasm_bindgen]
pub fn bundle_target(topology_json: &str, target_dir: &str) -> Result<Vec<u8>, JsError> {
    bundle_target_bytes(topology_json, target_dir).map_err(|e| JsError::new(&e.to_string()))
}

/// The download: a zip with `isoloom.yml` at the root, the project's own files where it points,
/// and each generated file at its path.
///
/// # Errors
///
/// A JavaScript `Error` saying why: the topology is malformed or doesn't compile, Isoloom finds
/// problems in the spec, no target generates any file for the project, or the zip can't be
/// written.
#[wasm_bindgen]
pub fn bundle(topology_json: &str) -> Result<Vec<u8>, JsError> {
    bundle_bytes(topology_json).map_err(|e| JsError::new(&e.to_string()))
}

/// JSON for a JavaScript caller: the project's own [`File`](crate::File)s, next to `isoloom.yml`
/// where its paths point (the appliances' configurations, the provisioning steps' starters).
///
/// # Errors
///
/// A JavaScript `Error` saying why: the topology is malformed or doesn't compile.
#[wasm_bindgen]
pub fn own_files_json(topology_json: &str) -> Result<String, JsError> {
    json(&own_files(topology_json).map_err(|e| JsError::new(&e.to_string()))?)
}

/// JSON for the caller: a [`Plan`](crate::Plan), one that says why it has nothing when the
/// topology gives none.
///
/// # Errors
///
/// A JavaScript `Error` if the JSON couldn't be written, which its values (strings, booleans,
/// nulls, lists and maps of them) never cause.
#[wasm_bindgen]
pub fn plan_json(topology_json: &str) -> Result<String, JsError> {
    json(&plan(topology_json))
}

/// JSON for the caller: Isoloom's lists ([`Choices`](crate::Choices)), what its OS, appliance and
/// protocol fields offer.
///
/// # Errors
///
/// A JavaScript `Error` if the JSON couldn't be written, which its values (strings, lists and
/// maps of them) never cause.
#[wasm_bindgen]
pub fn choices_json() -> Result<String, JsError> {
    json(&choices())
}

/// `value` as JSON. Every value handed out is strings, booleans, nulls, lists and maps of them
/// (no map with keys that aren't strings), so writing it
/// can't fail; were it to, the caller gets a JavaScript `Error` saying why, not a stand-in value.
fn json(value: &impl serde::Serialize) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|e| JsError::new(&format!("writing the JSON: {e}")))
}
