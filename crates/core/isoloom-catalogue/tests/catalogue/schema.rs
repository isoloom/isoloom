//! The generated schemas are current (`catalogue/*.schema.json`, `schemas/spec-v1.schema.json`)
//! and valid, and the spec's schema checks a machine's `with:` against its entry's settings.

use std::path::{Path, PathBuf};

use isoloom_catalogue::{Catalogue, schema};
use serde_json::Value;

const WRITE: &str = "cargo test -p isoloom-catalogue --test catalogue -- --ignored write_the_generated_schemas";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Each generated schema: its file, from the repository's root, and its content.
fn generated() -> Result<Vec<(&'static str, String)>, Box<dyn std::error::Error>> {
    let catalogue = Catalogue::embedded().map_err(ToString::to_string)?;
    let text = |v: Value| serde_json::to_string_pretty(&v).map(|t| t + "\n");
    Ok(vec![
        ("catalogue/entry.schema.json", text(schema::entry())?),
        ("catalogue/contract.schema.json", text(schema::contract())?),
        ("schemas/spec-v1.schema.json", text(schema::spec(catalogue))?),
    ])
}

#[test]
fn the_generated_schemas_are_current() {
    for (file, content) in generated().unwrap() {
        let on_disk = std::fs::read_to_string(root().join(file)).unwrap_or_default();
        assert!(on_disk == content, "{file} is out of date: run `{WRITE}`");
    }
}

/// Writes the generated schemas.
#[test]
#[ignore = "writes the generated schemas; run it after changing the types or the entries"]
fn write_the_generated_schemas() {
    for (file, content) in generated().unwrap() {
        std::fs::write(root().join(file), content).unwrap();
    }
}

#[test]
fn each_schema_is_a_valid_json_schema() {
    for (file, content) in generated().unwrap() {
        let v: Value = serde_json::from_str(&content).unwrap();
        assert!(jsonschema::validator_for(&v).is_ok(), "{file}");
    }
}

#[test]
fn the_entries_follow_their_schemas() {
    let entry = jsonschema::validator_for(&schema::entry()).unwrap();
    let contract = jsonschema::validator_for(&schema::contract()).unwrap();
    for e in Catalogue::embedded().unwrap().entries() {
        for (file, validator) in [("entry.yml", &entry), ("contract.yml", &contract)] {
            let text = std::fs::read_to_string(root().join("catalogue").join(e.name().as_str()).join(file)).unwrap();
            let v: Value = serde_yaml_ng::from_str(&text).unwrap();
            let errors: Vec<String> = validator.iter_errors(&v).map(|err| err.to_string()).collect();
            assert!(errors.is_empty(), "{}/{file}: {errors:?}", e.name());
        }
    }
}

#[test]
fn the_spec_schema_checks_with_against_the_entry_settings() {
    let catalogue = Catalogue::embedded().unwrap();
    let v = jsonschema::validator_for(&schema::spec(catalogue)).unwrap();
    let lab = |with: &str| -> Value {
        serde_yaml_ng::from_str(&format!("version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, service: nginx@1.30, with: {with}, networks: {{ lab: 10 }} }}\n")).unwrap()
    };
    assert!(v.is_valid(&lab("{ port: 8080, root: site/ }")));
    assert!(!v.is_valid(&lab("{ prot: 8080 }")), "a key outside the settings");
    assert!(!v.is_valid(&lab("{ port: http }")), "a port that isn't a number");
}

#[test]
fn a_password_setting_takes_a_literal_a_secret_or_an_input() {
    let catalogue = crate::testing::demo().unwrap();
    let with = schema::with(catalogue.entry("demo").unwrap());
    let v = jsonschema::validator_for(&with).unwrap();
    for ok in [
        serde_json::json!({ "admin": "hunter2" }),
        serde_json::json!({ "admin": { "secret": "a" } }),
        serde_json::json!({ "admin": { "input": "A" } }),
    ] {
        assert!(v.is_valid(&ok), "{ok}");
    }
    assert!(!v.is_valid(&serde_json::json!({ "admin": { "vault": "a" } })));
    assert!(!v.is_valid(&serde_json::json!({ "admin": 3 })));
}
