//! The coverage table stays honest: every field of the format has a row, every ✓ or ◐ names
//! an example that uses the field and generates for that output, and docs/COVERAGE.md is
//! what `isoloom coverage --markdown` prints.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use isoloom_core::coverage::{Output, Status, markdown, paths, table};
use isoloom_core::{generate, load, parse};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// A spec with every field set. Serialized back, unset fields appear too (as null), so a field
/// added to the model shows up here without editing this text.
const EVERY_FIELD: &str = r#"
version: 1
name: every-field
networks:
  out: { cidr: 10.1.0.0/24, internet: true }
  dmz: { cidr: 10.1.1.0/24, internet: false, gateway: fw }
reach: [{ from: out, to: dmz, ports: [80] }]
inputs: [TOKEN]
machines:
  fw:
    networks: { out: 2, dmz: 1 }
    services: [{ port: 80, name: web, http: true }]
    inputs: [TOKEN]
    resources: { cpus: 1, memory_mb: 512, disk_gb: 10 }
    depends_on: []
    access: false
    docker: { image: a, init: [x.sh] }
    vm: { os: debian-12, provision: [x.sh] }
checks: [c.sh]
targets: [docker]
"#;

#[test]
fn every_field_of_the_format_has_a_row_and_every_row_is_a_field() {
    let spec = parse(EVERY_FIELD).expect("parses");
    let fields: BTreeSet<String> = paths(&serde_yaml_ng::to_value(&spec).unwrap()).into_iter().collect();
    let rows: BTreeSet<String> = table().iter().map(|r| r.path.to_string()).collect();
    let missing: Vec<_> = fields.difference(&rows).collect();
    let stale: Vec<_> = rows.difference(&fields).collect();
    assert!(missing.is_empty(), "fields with no coverage row: {missing:?}");
    assert!(stale.is_empty(), "coverage rows for fields that don't exist: {stale:?}");
}

#[test]
fn every_claim_is_exercised_by_an_example() {
    for row in table() {
        for o in Output::ALL {
            let proof = match row.status(o) {
                Status::Done { proof, .. } | Status::Partial { proof, .. } => proof,
                _ => continue,
            };
            let dir = examples().join(proof);
            let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(&std::fs::read_to_string(dir.join("isoloom.yml")).unwrap()).unwrap();
            assert!(
                paths(&raw).iter().any(|p| p == row.path),
                "{} on {}: example `{proof}` doesn't use the field",
                row.path,
                o.label()
            );
            let spec = load(&dir).unwrap();
            assert!(
                generate(&spec, o.target()).is_ok(),
                "{} on {}: example `{proof}` doesn't generate for it",
                row.path,
                o.label()
            );
        }
    }
}

#[test]
fn docs_coverage_is_up_to_date() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/COVERAGE.md");
    let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(
        on_disk == markdown(),
        "docs/COVERAGE.md is stale; run `cargo run -q -- coverage --markdown > docs/COVERAGE.md`"
    );
}
