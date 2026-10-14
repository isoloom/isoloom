//! The Kubernetes, Kustomize and Helm tables against what the generator writes: every example
//! that Kubernetes doesn't refuse is generated, and each manifest, kustomization and chart is
//! followed down the pinned schemas. Every field written must have a row marked as written, and
//! every row marked as written must be written for some example.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // a failing test is the report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use isoloom_coverage::kubernetes::schema::{Schema, key};
use isoloom_coverage::{Format, helm, kubernetes, kustomize};
use isoloom_spec::model::Target;
use isoloom_spec::{load, parse};
use isoloom_targets::generate;
use serde_yaml_ng::Value;

#[path = "support/storage.rs"]
mod storage;

const CHART: &str = ".isoloom/kubernetes/chart/";

/// What no example sets but the generator writes: a machine's environment, sysctls, resolver
/// options, secrets, files and GPUs, and its own process (a command with `before` steps and a
/// working directory, a page as its healthcheck).
const FIXTURE: &str = r#"
version: 1
name: kubernetes-fixture
networks: { lan: { cidr: 10.9.0.0/24 } }
secrets:
  db-password: { env: DB_PASSWORD }
machines:
  app:
    networks: { lan: 10 }
    docker: { image: "alpine:3.20" }
    environment: { MODE: lab }
    sysctls: { net.ipv4.ip_unprivileged_port_start: "0" }
    dns: { options: ["ndots:2"] }
    secrets: [{ source: db-password, target: /etc/app/db-password, mode: 0400 }]
    files:
      - { path: /etc/app/app.ini, content: "mode=lab\n", uid: 101, mode: "0640" }
      - { path: /etc/app/site.conf, source: conf/site.conf }
  web:
    networks: { lan: 11 }
    services: [{ port: 8000 }]
    docker: { image: "python:3-alpine" }
    command: [python3, -m, http.server, "8000"]
    working_dir: /srv
    before: [flag.sh]
    healthcheck: { http: { port: 8000, path: / }, interval: 2s, timeout: 1s, retries: 30, start_period: 1s }
    gpus: { count: 1, capabilities: [compute] }
"#;

/// Every example's Kubernetes files, as `(path, contents)`, from the examples Kubernetes takes,
/// and the fixture's.
fn generated() -> Vec<(String, String)> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(examples)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("isoloom.yml").exists())
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let spec = load(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        if let Ok(files) = generate(&spec, Target::Kubernetes) {
            out.extend(files.into_iter().map(|f| (f.path, f.contents)));
        }
    }
    assert!(out.iter().any(|(p, _)| p.ends_with("environment.yaml")), "no example generates for Kubernetes");
    for text in [FIXTURE, storage::KUBERNETES] {
        let fixture = generate(&parse(text).unwrap(), Target::Kubernetes).unwrap();
        out.extend(fixture.into_iter().map(|f| (f.path, f.contents)));
    }
    out
}

/// Records the fields `value` sets, as the definition `definition` of `schema` reads them, and
/// follows each into the definition it holds.
fn walk(schema: &Schema, definition: &str, value: &Value, written: &mut BTreeSet<String>) {
    let Some(map) = value.as_mapping() else { return };
    if schema.fields_of(definition).next().is_none() {
        return; // a scalar definition (`Quantity`, `IntOrString`) or a free-form object
    }
    for (k, v) in map {
        let name = k.as_str().unwrap();
        let field = schema
            .field(definition, name)
            .unwrap_or_else(|| panic!("`{name}` isn't a field of {definition} in the pinned schema"));
        written.insert(key(definition, name));
        let target = field.target();
        if field.is_list() {
            for item in v.as_sequence().into_iter().flatten() {
                walk(schema, target, item, written);
            }
        } else if field.is_map() {
            for item in v.as_mapping().into_iter().flat_map(|m| m.values()) {
                walk(schema, target, item, written);
            }
        } else {
            walk(schema, target, v, written);
        }
    }
}

/// Every YAML document of a file.
fn documents(text: &str) -> Vec<Value> {
    serde_yaml_ng::Deserializer::from_str(text)
        .map(|d| serde::Deserialize::deserialize(d).unwrap())
        .filter(|v: &Value| !v.is_null())
        .collect()
}

/// A format's rows against the fields written: each written one has a row marked as written, and
/// each row marked as written is written.
fn assert_written(format: &Format, written: &BTreeSet<String>) {
    let mut problems = Vec::new();
    for k in written {
        match format.rows.iter().find(|(r, _)| r == k) {
            None => problems.push(format!("Isoloom writes `{k}`, which has no row (table its definition)")),
            Some((_, s)) if !s.written() => problems.push(format!("Isoloom writes `{k}`, but its row says {}", s.kind())),
            Some(_) => {}
        }
    }
    for (k, s) in &format.rows {
        if s.written() && !written.contains(k) {
            problems.push(format!("`{k}` is marked as written, but no example's output has it"));
        }
    }
    assert!(problems.is_empty(), "{}:\n{}", format.name, problems.join("\n"));
}

/// The objects of the manifests, the checks' Jobs and the chart's templates.
fn objects(files: &[(String, String)]) -> Vec<Value> {
    files
        .iter()
        .filter(|(p, _)| {
            let name = p.rsplit('/').next().unwrap_or_default();
            Path::new(p).extension().is_some_and(|e| e == "yaml") && !["kustomization.yaml", "Chart.yaml", "values.yaml"].contains(&name)
        })
        .flat_map(|(_, text)| documents(text))
        .collect()
}

#[test]
fn kubernetes_written_fields_match_the_generated_manifests() {
    let schema = kubernetes::openapi();
    let mut written = BTreeSet::new();
    for object in objects(&generated()) {
        let name = object["kind"].as_str().unwrap();
        let kind = schema
            .kind(name)
            .unwrap_or_else(|| panic!("Isoloom writes a {name}, which isn't a kind of the pinned OpenAPI"));
        assert_eq!(object["apiVersion"].as_str(), Some(kind.api_version), "{name}");
        written.insert(key(kubernetes::KINDS, name));
        walk(&schema, kind.definition, &object, &mut written);
    }
    assert_written(&kubernetes::format(), &written);
}

#[test]
fn kustomize_written_fields_match_the_generated_kustomizations() {
    let schema = kustomize::schema();
    let mut written = BTreeSet::new();
    for (_, text) in generated().iter().filter(|(p, _)| p.ends_with("kustomization.yaml")) {
        for doc in documents(text) {
            walk(&schema, "Kustomization", &doc, &mut written);
        }
    }
    assert_written(&kustomize::format(), &written);
}

/// A chart file's feature row: its path within the chart, matched against [`helm::CHART_FILES`].
fn chart_features(path: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = helm::CHART_FILES
        .iter()
        .copied()
        .filter(|f| if f.ends_with('/') { path.starts_with(f) } else { path == *f })
        .collect();
    if path.starts_with("templates/_") && Path::new(path).extension().is_some_and(|e| e == "tpl") {
        found.push("templates/_helpers.tpl");
    }
    if found.is_empty() {
        found.push(".Files");
    }
    found
}

#[test]
fn helm_written_features_match_the_generated_chart() {
    let schema = helm::schema();
    let files = generated();
    let mut written = BTreeSet::new();
    for (path, text) in &files {
        let Some(within) = path.split_once(CHART).map(|(_, w)| w) else { continue };
        for f in chart_features(within) {
            written.insert(key(helm::FILES, f));
        }
        if within == "Chart.yaml" {
            for doc in documents(text) {
                walk(&schema, "Chart", &doc, &mut written);
            }
        }
    }
    let charts: Vec<(String, String)> = files.into_iter().filter(|(p, _)| p.contains(CHART)).collect();
    for object in objects(&charts) {
        for (k, v) in object["metadata"]["annotations"].as_mapping().into_iter().flatten() {
            let k = k.as_str().unwrap();
            if k == "helm.sh/hook" {
                written.extend(v.as_str().unwrap().split(',').map(|h| key(helm::HOOKS, h.trim())));
            } else if k.starts_with("helm.sh/") {
                written.insert(key(helm::HOOKS, k));
            }
        }
    }
    assert_written(&helm::format(), &written);
}
