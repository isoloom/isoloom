//! The Compose table against its two sources of truth: the pinned compose-spec schema (every
//! key path is a row, and every row is a key path) and every Compose file Isoloom writes (the
//! `docker`, `hosted`, `docker-vm`, `cloud-docker` and `hybrid` targets, for every committed
//! example and the fixtures below). A key path written is a row marked as written, and every
//! row marked as written is written by some example or fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::BTreeSet;
use std::path::PathBuf;

use isoloom_coverage::compose;
use isoloom_spec::model::{Spec, Target};
use isoloom_spec::{load, parse};
use isoloom_targets::generate;
use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn schema() -> Json {
    let text = std::fs::read_to_string(crate_dir().join("coverage/compose-spec.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The concrete alternatives of a schema node: `$ref`, `allOf`, `anyOf` and `oneOf` followed
/// down to the nodes that carry `properties`, `patternProperties` or `items`.
fn alternatives<'a>(defs: &'a Json, node: &'a Json, out: &mut Vec<&'a Json>) {
    out.push(node);
    if let Some(r) = node["$ref"].as_str() {
        alternatives(defs, &defs[r.rsplit('/').next().unwrap()], out);
    }
    for combinator in ["allOf", "anyOf", "oneOf"] {
        for sub in node[combinator].as_array().into_iter().flatten() {
            alternatives(defs, sub, out);
        }
    }
}

/// What a node allows below it: named keys, entries under names the file chooses (`*`, only
/// when an entry has keys of its own), and list items (`[]`).
#[derive(Default)]
struct Children<'a> {
    keys: Vec<(&'a str, &'a Json)>,
    each: Vec<&'a Json>,
    items: Vec<&'a Json>,
}

fn children<'a>(defs: &'a Json, node: &'a Json) -> Children<'a> {
    let mut alts = Vec::new();
    alternatives(defs, node, &mut alts);
    let mut c = Children::default();
    for alt in alts {
        for (k, v) in alt["properties"].as_object().into_iter().flatten() {
            c.keys.push((k.as_str(), v));
        }
        let patterns = alt["patternProperties"].as_object().into_iter().flatten();
        // `x-` keys are the format's extension point: free-form, not features.
        c.each.extend(patterns.filter(|(p, _)| !p.starts_with("^x-")).map(|(_, v)| v));
        if alt["additionalProperties"].is_object() {
            c.each.push(&alt["additionalProperties"]);
        }
        if alt["items"].is_object() {
            c.items.push(&alt["items"]);
        }
    }
    c
}

/// Every key path the schema allows below `node`, `prefix` first, but none below `one_row`.
fn schema_paths(defs: &Json, node: &Json, prefix: &str, one_row: &[&str], out: &mut BTreeSet<String>) {
    if one_row.contains(&prefix) {
        return;
    }
    let c = children(defs, node);
    for (k, v) in c.keys {
        let path = if prefix.is_empty() { k.to_string() } else { format!("{prefix}.{k}") };
        out.insert(path.clone());
        schema_paths(defs, v, &path, one_row, out);
    }
    for v in c.each {
        schema_paths(defs, v, &format!("{prefix}.*"), one_row, out);
    }
    for v in c.items {
        schema_paths(defs, v, &format!("{prefix}[]"), one_row, out);
    }
}

#[test]
fn compose_rows_are_exactly_the_schema_key_paths() {
    let schema = schema();
    let mut expected = BTreeSet::new();
    schema_paths(&schema["$defs"], &schema, "", compose::ONE_ROW, &mut expected);
    let rows: BTreeSet<String> = compose::format().rows.iter().map(|(k, _)| k.clone()).collect();
    let missing: Vec<_> = expected.difference(&rows).collect();
    let extra: Vec<_> = rows.difference(&expected).collect();
    assert!(missing.is_empty(), "Compose key paths with no coverage row: {missing:?}");
    assert!(extra.is_empty(), "coverage rows that aren't Compose key paths: {extra:?}");
}

#[test]
fn compose_key_list_is_the_vendored_schema() {
    // The refresh script writes the list the drift job watches: it must be this schema's.
    let schema = schema();
    let mut expected = BTreeSet::new();
    schema_paths(&schema["$defs"], &schema, "", &[], &mut expected);
    let text = std::fs::read_to_string(crate_dir().join("coverage/compose-keys.txt")).unwrap();
    let listed: BTreeSet<String> = text.lines().map(str::to_string).collect();
    assert!(
        listed == expected,
        "coverage/compose-keys.txt is stale; run `sh scripts/refresh-compose-schema.sh`"
    );
}

/// The key paths of a written document, read through the schema so they name the rows
/// (`services.*.networks.*.ipv4_address`); a key the schema doesn't know is reported as is.
fn written_paths(defs: &Json, node: &Json, value: &Yaml, prefix: &str, out: &mut BTreeSet<String>) {
    let c = children(defs, node);
    match value {
        Yaml::Mapping(m) => {
            for (k, v) in m {
                let k = k.as_str().unwrap_or_default();
                if let Some((_, sub)) = c.keys.iter().find(|(name, _)| *name == k) {
                    let path = if prefix.is_empty() { k.to_string() } else { format!("{prefix}.{k}") };
                    out.insert(path.clone());
                    written_paths(defs, sub, v, &path, out);
                } else if let Some(sub) = c.each.first() {
                    written_paths(defs, sub, v, &format!("{prefix}.*"), out);
                } else if c.keys.is_empty() && c.each.is_empty() {
                    // A free-form map (labels, environment): its keys are values, not features.
                } else {
                    out.insert(format!("{prefix}.{k} (not in the schema)"));
                }
            }
        }
        Yaml::Sequence(items) => {
            for item in items {
                for sub in &c.items {
                    written_paths(defs, sub, item, &format!("{prefix}[]"), out);
                }
            }
        }
        _ => {}
    }
}

/// Every target whose output holds a Compose file.
#[path = "support/storage.rs"]
mod storage;

const COMPOSE_TARGETS: &[Target] = &[Target::Docker, Target::Hosted, Target::DockerVm, Target::CloudDocker, Target::Hybrid];

/// Specs that exercise what no committed example does, so every row marked as written is
/// proven by a generated file.
const FIXTURES: &[&str] = &[
    // An IPv6-only network: `networks.*.enable_ipv4`.
    r#"
version: 1
name: ipv6-only
networks: { lab: { cidr: 10.9.1.0/24, internet: false, ipv6: { cidr: "fd00:9:1::/64", only: true } } }
machines:
  web: { networks: { lab: 10 }, docker: { image: nginx:alpine } }
"#,
    // A tool with its own command: `services.*.command`.
    r#"
version: 1
name: tool-command
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lan: 10 }, docker: { image: nginx:alpine } }
tools: { viewer: { image: python:3-alpine, command: [python3, -m, http.server, "8000"], port: 8000, publish: 9000 } }
"#,
    // Resolver options and a build stage: `services.*.dns_opt`, `services.*.build.target`.
    r#"
version: 1
name: dns-and-stage
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    dns: { options: ["ndots:2", "timeout:1"] }
    docker: { build: app, target: lab }
"#,
    // Secrets: the top-level `secrets` and `services.*.secrets` with a path, owner and mode.
    r"
version: 1
name: secrets
networks: { lan: { cidr: 10.9.0.0/24 } }
secrets:
  db-password: { env: DB_PASSWORD }
  tls-key: { file: tls.key }
machines:
  web:
    networks: { lan: 10 }
    docker: { image: nginx:alpine }
    secrets: [db-password, { source: tls-key, target: /etc/ssl/app.key, uid: 101, gid: 101, mode: 0400 }]
",
    // An existing Docker network, joined by name: `networks.*.external`.
    r"
version: 1
name: corp-join
networks: { lan: { cidr: 10.9.0.0/24 }, corp: { cidr: 10.77.0.0/24, external: corp-net } }
machines:
  web: { networks: { lan: 10, corp: 10 }, docker: { image: nginx:alpine } }
",
    // A machine's files: `configs`, as text with an owner and mode, as a project file, and as
    // a project file with an owner and mode (`configs.*.environment`).
    r#"
version: 1
name: machine-files
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    docker: { image: nginx:alpine }
    files:
      - { path: /etc/app/app.ini, content: "mode=lab\n", uid: 101, gid: 101, mode: "0640" }
      - { path: /etc/nginx/conf.d/site.conf, source: conf/site.conf }
      - { path: /etc/app/key.pem, source: conf/key.pem, uid: 101, mode: "0400" }
"#,
    // A machine's own process: `command`, `working_dir`, `healthcheck.test` (`before` in the
    // entrypoint).
    r#"
version: 1
name: machine-command
inputs: [FLAG]
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    inputs: [FLAG]
    services: [{ port: 8000 }]
    docker: { image: python:3-alpine }
    command: [python3, -m, http.server, "8000"]
    working_dir: /srv
    before: [flag.sh]
    healthcheck: { test: "test -s /srv/flag", interval: 2s, retries: 30 }
"#,
    // GPUs and the host's devices: `deploy.resources.reservations.devices` and `devices`.
    r#"
version: 1
name: hardware
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  gpu: { networks: { lan: 10 }, docker: { image: nginx:alpine }, gpus: { count: all, capabilities: [compute], options: { virtualization: "false" } } }
  pinned: { networks: { lan: 11 }, docker: { image: nginx:alpine }, gpus: { ids: ["0"], driver: nvidia } }
  fuse: { networks: { lan: 12 }, docker: { image: nginx:alpine }, devices: [{ host: /dev/fuse, guest: /dev/fuse, permissions: rw }] }
"#,
    // Volumes kept past `down` or existing, a folder of a shared volume, a sized tmpfs, a disk.
    storage::CONTAINERS,
];

/// Every Compose file Isoloom writes for `spec`, on every target that writes one.
fn compose_files(spec: &Spec) -> Vec<String> {
    let mut found = Vec::new();
    for target in COMPOSE_TARGETS {
        let Ok(files) = generate(spec, *target) else { continue };
        found.extend(files.into_iter().filter(|f| f.path.ends_with("compose.yml")).map(|f| f.contents));
    }
    found
}

fn every_spec() -> Vec<Spec> {
    let examples = crate_dir().join("../../examples");
    let mut specs: Vec<Spec> = Vec::new();
    for entry in std::fs::read_dir(&examples).unwrap() {
        let dir = entry.unwrap().path();
        if dir.join("isoloom.yml").is_file() {
            specs.push(load(&dir).unwrap());
        }
    }
    specs.extend(FIXTURES.iter().map(|f| parse(f).unwrap()));
    specs
}

#[test]
fn compose_written_key_paths_match_the_generated_files() {
    let schema = schema();
    let defs = &schema["$defs"];
    let mut written = BTreeSet::new();
    let mut targets_seen = BTreeSet::new();
    for spec in every_spec() {
        for text in compose_files(&spec) {
            let doc: Yaml = serde_yaml_ng::from_str(&text).unwrap();
            written_paths(defs, &schema, &doc, "", &mut written);
        }
        for target in COMPOSE_TARGETS {
            if generate(&spec, *target).is_ok() {
                targets_seen.insert(format!("{target:?}"));
            }
        }
    }
    assert_eq!(
        targets_seen.len(),
        COMPOSE_TARGETS.len(),
        "some Compose target generates for no example: {targets_seen:?}"
    );
    let rows = compose::format().rows;
    let mut problems = Vec::new();
    for key in &written {
        match rows.iter().find(|(k, _)| k == key) {
            None => problems.push(format!("Isoloom writes `{key}`, which has no coverage row")),
            Some((_, s)) if !s.written() => problems.push(format!("Isoloom writes `{key}`, but its row says {}", s.kind())),
            Some(_) => {}
        }
    }
    for (key, s) in &rows {
        if s.written() && !written.contains(key) {
            problems.push(format!("`{key}` is marked as written, but no example's or fixture's output has it"));
        }
    }
    assert!(problems.is_empty(), "Compose:\n{}", problems.join("\n"));
}

#[test]
fn compose_rows_follow_their_parents() {
    // A nested row sits right after its parent's rows, so the page reads as a tree.
    let rows = compose::format().rows;
    for (i, (key, _)) in rows.iter().enumerate() {
        let parent = key.rsplit_once('.').map(|(p, _)| p.trim_end_matches(".*").trim_end_matches("[]"));
        let Some(parent) = parent.filter(|p| rows.iter().any(|(k, _)| k == p)) else {
            continue;
        };
        let at = rows.iter().position(|(k, _)| k == parent).unwrap();
        assert!(at < i, "`{key}` comes before its parent `{parent}`");
        assert!(
            !parent.contains('.') || rows[at..i].iter().all(|(k, _)| k.starts_with(parent)),
            "`{key}` is separated from its parent `{parent}`"
        );
    }
}
