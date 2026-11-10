//! The committed JSON Schema is the one the types generate, and it agrees with them on the
//! fixtures.

use std::path::PathBuf;

const WRITE: &str = "cargo test -p isoloom-manifest --test schema -- --ignored write_the_committed_schema";

fn committed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../schemas/run-manifest-v1.schema.json")
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
        "schemas/run-manifest-v1.schema.json is out of date: run `{WRITE}`"
    );
}

/// Writes the schema the types generate to `schemas/run-manifest-v1.schema.json`.
#[test]
#[ignore = "writes the committed schema; run it after changing the types"]
fn write_the_committed_schema() {
    let path = committed();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, generated().unwrap()).unwrap();
}

/// What the schema says calls the environment an environment, as every user-facing text does:
/// never a lab (a word in a pattern or an example name aside).
#[test]
fn the_schema_says_environment() {
    let text = generated().unwrap();
    let words = text.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'');
    let lab: Vec<&str> = words.filter(|w| matches!(w.to_ascii_lowercase().as_str(), "lab" | "labs" | "lab's")).collect();
    assert!(lab.is_empty(), "the schema says {lab:?}");
}

#[test]
fn the_schema_is_a_valid_json_schema() {
    jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
}
