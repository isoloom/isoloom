//! The shared fixtures, the contract both Isoloom and the runner test against: every manifest
//! under `fixtures/` parses, validates against the schema and writes back to the same value; every
//! one under `fixtures/invalid/` is refused with the error its `.error.json` names, and by the
//! schema too when that file says the schema can tell.

use std::error::Error;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The manifests in the fixtures folder `dir`, in name order.
fn fixtures(dir: &str) -> Outcome<Vec<PathBuf>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(dir);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.to_string_lossy().ends_with(".run.json") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// A validator of the generated schema.
fn validator() -> Outcome<jsonschema::Validator> {
    Ok(jsonschema::validator_for(&isoloom_manifest::schema())?)
}

#[test]
fn every_valid_fixture_parses_validates_and_writes_back() -> Outcome {
    let schema = validator()?;
    let valid = fixtures("")?;
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
    Ok(())
}

#[test]
fn every_invalid_fixture_gives_its_error() -> Outcome {
    let schema = validator()?;
    let invalid = fixtures("invalid")?;
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
    Ok(())
}
