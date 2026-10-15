//! The run manifest the `proxmox` target writes (`.isoloom/proxmox/run.json`): the solo-web
//! example writes the shared fixture the runner tests against, and every example Proxmox takes
//! writes one that parses and matches the schema.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_manifest::{Manifest, Version};
use isoloom_spec::{Target, load};
use isoloom_targets::{Lab, generate};

const RUN_JSON: &str = ".isoloom/proxmox/run.json";

fn example(name: &str) -> Lab {
    Lab::new(load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name)).unwrap())
}

fn manifest(lab: &Lab) -> Manifest {
    let files = generate(lab, Target::Proxmox).unwrap();
    let file = files.iter().find(|f| f.path == RUN_JSON).unwrap_or_else(|| panic!("no {RUN_JSON}"));
    isoloom_manifest::parse(&file.contents).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn solo_web_writes_the_shared_proxmox_fixture() {
    let mut generated = manifest(&example("solo-web"));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/isoloom-manifest/fixtures/proxmox.run.json");
    let mut fixture = isoloom_manifest::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    // A release changes the version, not the contract.
    generated.generator.version = Version::parse("0.0.0").unwrap();
    fixture.generator.version = Version::parse("0.0.0").unwrap();
    assert_eq!(generated, fixture);
}

#[test]
fn every_proxmox_examples_manifest_parses_and_matches_the_schema() {
    let schema = jsonschema::validator_for(&isoloom_manifest::schema()).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut seen = 0;
    for dir in std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("isoloom.yml").is_file())
    {
        let Ok(files) = generate(&Lab::new(load(&dir).unwrap()), Target::Proxmox) else {
            continue;
        };
        let file = files
            .iter()
            .find(|f| f.path == RUN_JSON)
            .unwrap_or_else(|| panic!("{}: no run.json", dir.display()));
        isoloom_manifest::parse(&file.contents).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let doc: serde_json::Value = serde_json::from_str(&file.contents).unwrap();
        let errors: Vec<String> = schema.iter_errors(&doc).map(|e| format!("{e} at {}", e.instance_path())).collect();
        assert!(errors.is_empty(), "{}: {errors:#?}", dir.display());
        seen += 1;
    }
    assert!(seen >= 10, "{seen}");
}
