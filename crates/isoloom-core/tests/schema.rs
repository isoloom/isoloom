//! The JSON Schema editors use agrees with the parser: every example is valid, typos are
//! caught, and the examples point editors to it.

use std::path::Path;

use isoloom_core::schema::{MODELINE, schema};

fn examples() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter_map(|p| {
            Some((
                p.file_name()?.to_string_lossy().to_string(),
                std::fs::read_to_string(p.join("isoloom.yml")).ok()?,
            ))
        })
        .collect()
}

fn validator() -> jsonschema::Validator {
    jsonschema::validator_for(&schema()).expect("the schema is a valid JSON Schema")
}

#[test]
fn every_example_is_valid_against_the_schema() {
    let v = validator();
    for (name, text) in examples() {
        let doc: serde_json::Value = serde_yaml_ng::from_str(&text).unwrap();
        let errors: Vec<String> = v.iter_errors(&doc).map(|e| format!("{} at {}", e, e.instance_path())).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}

#[test]
fn the_schema_catches_what_editors_should_flag() {
    let v = validator();
    for bad in [
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, dockr: { image: x } } }\n",
        "version: 2\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, docker: { image: x } } }\n",
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, vm: { os: windows-95 } } }\n",
    ] {
        let doc: serde_json::Value = serde_yaml_ng::from_str(bad).unwrap();
        assert!(!v.is_valid(&doc), "should be flagged: {bad}");
    }
}

#[test]
fn examples_point_editors_to_the_schema() {
    for (name, text) in examples() {
        assert_eq!(text.lines().next(), Some(MODELINE), "{name}: first line should be the schema modeline");
    }
}
