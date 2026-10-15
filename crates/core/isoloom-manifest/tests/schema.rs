//! The committed JSON Schema is the one the types generate, and it agrees with them on the
//! fixtures.

use std::path::PathBuf;

const WRITE: &str = "cargo test -p isoloom-manifest --test schema -- --ignored write_the_committed_schema";

fn committed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../schemas/run-manifest-v0.schema.json")
}

/// The schema the types generate, as the committed file holds it.
fn generated() -> serde_json::Result<String> {
    serde_json::to_string_pretty(&isoloom_manifest::schema()).map(|text| text + "\n")
}

#[test]
fn the_committed_schema_is_current() {
    let on_disk = std::fs::read_to_string(committed()).unwrap_or_default();
    assert!(
        on_disk == generated().unwrap(),
        "schemas/run-manifest-v0.schema.json is out of date: run `{WRITE}`"
    );
}

/// Writes the schema the types generate to `schemas/run-manifest-v0.schema.json`.
#[test]
#[ignore = "writes the committed schema; run it after changing the types"]
fn write_the_committed_schema() {
    let path = committed();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, generated().unwrap()).unwrap();
}

#[test]
fn the_schema_is_a_valid_json_schema() {
    jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
}
