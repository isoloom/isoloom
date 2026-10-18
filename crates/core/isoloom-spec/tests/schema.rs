//! The JSON Schema editors use agrees with the parser: every example is valid, typos are
//! caught, the examples point editors to it, and the committed copy
//! (`schemas/spec-v1.schema.json`) is the one the types generate.

use std::error::Error;
use std::path::Path;

use isoloom_spec::schema::{MODELINE, schema};

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The committed schema is written by the catalogue, which adds each entry's `with:` rule.
const WRITE: &str = "cargo test -p isoloom-catalogue --test catalogue -- --ignored write_the_generated_schemas";

/// The committed copy of the schema.
fn committed() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../schemas/spec-v1.schema.json")
}

#[test]
fn the_committed_schema_is_current() {
    let on_disk: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(committed()).unwrap_or_default()).unwrap_or_default();
    // The catalogue adds one rule per entry (`with:` follows its settings); the rest is the
    // spec's own.
    let mut own = on_disk;
    if let Some(machine) = own.pointer_mut("/$defs/Machine").and_then(serde_json::Value::as_object_mut) {
        machine.remove("allOf");
    }
    assert!(own == schema(), "schemas/spec-v1.schema.json is out of date: run `{WRITE}`");
}

/// Every example of the project: its folder's name and its spec's text.
fn examples() -> Outcome<Vec<(String, String)>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if let (Some(name), Ok(text)) = (path.file_name(), std::fs::read_to_string(path.join("isoloom.yml"))) {
            out.push((name.to_string_lossy().into_owned(), text));
        }
    }
    Ok(out)
}

/// A validator of the spec's schema.
fn validator() -> Outcome<jsonschema::Validator> {
    Ok(jsonschema::validator_for(&schema())?)
}

#[test]
fn every_example_is_valid_against_the_schema() -> Outcome {
    let v = validator()?;
    for (name, text) in examples()? {
        let doc: serde_json::Value = serde_yaml_ng::from_str(&text).unwrap();
        let errors: Vec<String> = v.iter_errors(&doc).map(|e| format!("{} at {}", e, e.instance_path())).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
    Ok(())
}

#[test]
fn the_schema_catches_what_editors_should_flag() -> Outcome {
    let v = validator()?;
    for bad in [
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, ap: { image: x } } }\n",
        "version: 2\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, app: { image: x } } }\n",
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, docker: { image: x } } }\n",
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, os: windows-95 } }\n",
        "version: 1\nname: x\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lab: 5 }, os: debian@12, ports: [{ port: 80 }] } }\n",
    ] {
        let doc: serde_json::Value = serde_yaml_ng::from_str(bad).unwrap();
        assert!(!v.is_valid(&doc), "should be flagged: {bad}");
    }
    Ok(())
}

#[test]
fn examples_point_editors_to_the_schema() -> Outcome {
    for (name, text) in examples()? {
        assert_eq!(text.lines().next(), Some(MODELINE), "{name}: first line should be the schema modeline");
    }
    Ok(())
}
