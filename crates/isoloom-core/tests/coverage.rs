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
  out: { cidr: 10.1.0.0/24, internet: true, docker: { cidr: 10.1.0.0/24 } }
  dmz: { cidr: 10.1.1.0/24, internet: false, gateway: fw, docker: { cidr: 10.1.1.0/24 } }
  far: { cidr: 10.1.2.0/24, tc: { delay: 50ms, jitter: 5ms, loss: 0.5, rate: 10mbit } }
  office: { cidr: 10.2.0.0/16, docker: { cidr: 10.2.0.0/16 }, switch: fw, vlans: { 10: { cidr: 10.2.10.0/24, internet: false } } }
reach: [{ from: out, to: dmz, ports: [80] }]
inputs: [TOKEN]
machines:
  fw:
    networks: { out: 2, dmz: 1 }
    services: [{ port: 80, name: web, http: true, tls: true, publish: 8080, fixed: true }]
    inputs: [TOKEN]
    arch: amd64
    privileged: true
    read_only: true
    tmpfs: [/tmp, /run]
    shm_size: 256m
    dns: { servers: [10.0.0.1], search: [lab.local], domain: lab.local }
    resources: { cpus: 1, memory_mb: 512, disk_gb: 10 }
    depends_on: []
    volumes: { data: /data }
    access: false
    aliases: [api.example.test]
    docker: { image: a, init: [x.sh], idle: true, appliance: cisco-iol, config: r.cfg, firmware: f.bin }
    vm: { os: debian-12, provision: [x.sh], image: { vagrant: x/y, vagrant_version: "1", winrm: ssl } }
    external: { address: 192.168.1.2, user: admin, port: 2222, key: ~/.ssh/lab }
  h:
    count: 2
    networks: { out: 10 }
    docker: { build: h, dockerfile: h.Dockerfile, args: { A: b } }
provision: [{ ansible: site.yml, inventory: [inv.ini], groups: { dc: [fw] }, vars: { a: b }, requirements: req.yml }]
common: { resources: { cpus: 1 } }
groups: { g: { members: [fw], arch: amd64 } }
checks: [c.sh, { name: n, from: fw, http: "http://x/", method: POST, headers: { A: b }, body: x, contains: y, tcp: "x:1", exec: e, script: s.sh, expect: 200, wait: 1 }]
targets: [docker]
message: hello
tools: { shell: {}, viewer: { image: x, command: [serve], port: 80, publish: 9000 } }
"#;

#[test]
fn every_field_of_the_format_has_a_row_and_every_row_is_a_field() {
    parse(EVERY_FIELD).expect("parses");
    // The document as written: `count`, `common`, `groups` and `vlans` are folded away by parsing.
    let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(EVERY_FIELD).unwrap();
    let fields: BTreeSet<String> = paths(&raw).into_iter().collect();
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

// From the formats' side.

use isoloom_core::coverage::{Support, compose, formats, vagrant};

/// Every committed output of a generator, across the examples.
fn outputs(file: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(examples()).unwrap() {
        let p = entry.unwrap().path().join(file);
        if let Ok(text) = std::fs::read_to_string(p) {
            found.push(text);
        }
    }
    assert!(!found.is_empty(), "no example has {file}");
    found
}

/// The property names a schema node allows, following `$ref` and `allOf`/`anyOf`/`oneOf`.
fn schema_keys(defs: &serde_json::Value, node: &serde_json::Value) -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = node["properties"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    if let Some(r) = node["$ref"].as_str() {
        keys.extend(schema_keys(defs, &defs[r.rsplit('/').next().unwrap()]));
    }
    for combinator in ["allOf", "anyOf", "oneOf"] {
        for sub in node[combinator].as_array().into_iter().flatten() {
            keys.extend(schema_keys(defs, sub));
        }
    }
    keys
}

#[test]
fn compose_rows_are_exactly_the_schema_keys() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("coverage/compose-spec.json")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&text).unwrap();
    let defs = &schema["$defs"];
    let mut expected: BTreeSet<String> = schema_keys(defs, &schema);
    for (prefix, def) in [("services.*.", "service"), ("networks.*.", "network"), ("volumes.*.", "volume")] {
        expected.extend(schema_keys(defs, &defs[def]).into_iter().map(|k| format!("{prefix}{k}")));
    }
    let rows: BTreeSet<String> = compose::format().rows.iter().map(|(k, _)| k.to_string()).collect();
    let missing: Vec<_> = expected.difference(&rows).collect();
    let extra: Vec<_> = rows.difference(&expected).collect();
    assert!(missing.is_empty(), "Compose keys with no coverage row: {missing:?}");
    assert!(extra.is_empty(), "coverage rows that aren't Compose keys: {extra:?}");
}

/// Checks a format's rows against what the generator really writes: every key written has a
/// row marked as written, and every row marked as written is written by some example.
fn written_matches(rows: &[(String, Support)], written: BTreeSet<String>, format: &str) {
    for key in &written {
        let row = rows.iter().find(|(k, _)| k == key);
        assert!(row.is_some(), "{format}: Isoloom writes `{key}`, which has no coverage row");
        assert!(
            row.unwrap().1.written(),
            "{format}: Isoloom writes `{key}`, but its row says {}",
            row.unwrap().1.kind()
        );
    }
    for (key, s) in rows {
        if s.written() {
            assert!(written.contains(key), "{format}: `{key}` is marked as written, but no example's output has it");
        }
    }
}

#[test]
fn compose_written_keys_match_the_generated_files() {
    let mut written = BTreeSet::new();
    for text in outputs(".isoloom/docker/compose.yml") {
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        for (k, v) in doc.as_mapping().unwrap() {
            let k = k.as_str().unwrap();
            written.insert(k.to_string());
            if let ("services" | "networks" | "volumes", Some(items)) = (k, v.as_mapping()) {
                for item in items.values() {
                    for key in item.as_mapping().into_iter().flat_map(|m| m.keys()) {
                        written.insert(format!("{k}.*.{}", key.as_str().unwrap()));
                    }
                }
            }
        }
    }
    written_matches(&compose::format().rows, written, "Compose");
}

#[test]
fn vagrant_written_settings_match_the_generated_files() {
    let mut written = BTreeSet::new();
    for text in outputs(".isoloom/vagrant/Vagrantfile") {
        let mut provider: Option<String> = None;
        for line in text.lines() {
            let line = line.trim_start();
            // Inside `m.vm.provider "x" do |v|`: every `v.setting` is that provider's.
            if let Some(p) = &provider {
                if line == "end" {
                    provider = None;
                } else if let Some(rest) = line.strip_prefix("v.") {
                    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    written.insert(format!("provider {p}: {name}"));
                }
            }
            // `config.vm.x`, `m.vm.x`, `o.vm.x`: a machine setting; network, provision and
            // provider settings also name their type.
            if line.starts_with("m.winrm.") || line.starts_with("config.winrm.") {
                written.insert("config.winrm".to_string());
                continue;
            }
            let Some(rest) = ["config.vm.", "m.vm.", "o.vm."].iter().find_map(|p| line.strip_prefix(p)) else {
                continue;
            };
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let kind = rest.split('"').nth(1).unwrap_or_default();
            match name.as_str() {
                "network" | "provision" => {
                    written.insert(format!("config.vm.{name} {kind}"));
                    written.insert(format!("config.vm.{name}"));
                }
                "provider" => {
                    written.insert("config.vm.provider".to_string());
                    provider = Some(kind.to_string());
                }
                _ => {
                    written.insert(format!("config.vm.{name}"));
                }
            }
        }
    }
    let rows: Vec<(String, Support)> = vagrant::formats().into_iter().flat_map(|f| f.rows).collect();
    written_matches(&rows, written, "Vagrant");
}

#[test]
fn every_feature_is_classified() {
    for f in formats() {
        let unclassified: Vec<_> = f.rows.iter().filter(|(_, s)| *s == Support::Unclassified).map(|(k, _)| k.as_str()).collect();
        assert!(unclassified.is_empty(), "{}: classify these in coverage/: {unclassified:?}", f.name);
        let mut keys = BTreeSet::new();
        for (k, _) in &f.rows {
            assert!(keys.insert(k), "{}: `{k}` is listed twice", f.name);
        }
    }
}

#[test]
fn every_cloud_resource_named_exists_in_its_provider() {
    for c in isoloom_core::coverage::terraform::CLOUDS {
        let all = vagrant::names(c.list);
        for r in c.environment.iter().chain(c.windows) {
            assert!(all.contains(r), "{}: `{r}` isn't a resource type of {}", c.name, c.source);
        }
    }
}
