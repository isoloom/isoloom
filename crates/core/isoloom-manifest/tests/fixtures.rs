//! The shared fixtures, the contract both Isoloom and the runner test against: every manifest
//! under `fixtures/` parses, validates against the schema and writes back to the same value; every
//! one under `fixtures/invalid/` is refused with the error its `.error.json` names, and by the
//! schema too when that file says the schema can tell.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

fn fixtures(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(dir);
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".run.json"))
        .collect();
    out.sort();
    out
}

fn validator() -> jsonschema::Validator {
    jsonschema::validator_for(&isoloom_manifest::schema()).unwrap()
}

#[test]
fn every_valid_fixture_parses_validates_and_writes_back() {
    let schema = validator();
    let valid = fixtures("");
    assert!(valid.len() >= 3, "{valid:?}");
    for path in valid {
        let text = std::fs::read_to_string(&path).unwrap();
        let manifest = isoloom_manifest::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let doc: Value = serde_json::from_str(&text).unwrap();
        let errors: Vec<String> = schema.iter_errors(&doc).map(|e| format!("{} at {}", e, e.instance_path())).collect();
        assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
        let written = isoloom_manifest::to_json(&manifest).unwrap();
        assert_eq!(isoloom_manifest::parse(&written).unwrap(), manifest, "{}", path.display());
        assert!(schema.is_valid(&serde_json::from_str(&written).unwrap()), "{}: written back", path.display());
    }
}

#[test]
fn every_invalid_fixture_gives_its_error() {
    let schema = validator();
    let invalid = fixtures("invalid");
    assert!(invalid.len() >= 15, "{invalid:?}");
    for path in invalid {
        let text = std::fs::read_to_string(&path).unwrap();
        let err = isoloom_manifest::parse(&text).expect_err(&path.display().to_string());
        let doc: Value = serde_json::from_str(&text).unwrap();
        let actual = json!({ "code": err.code(), "message": err.to_string(), "schema": !schema.is_valid(&doc) });
        let expected_path = PathBuf::from(path.to_string_lossy().replace(".run.json", ".error.json"));
        let expected: Value = std::fs::read_to_string(&expected_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| panic!("{}: missing; it would hold:\n{actual:#}", expected_path.display()));
        assert_eq!(actual, expected, "{}", path.display());
    }
}
