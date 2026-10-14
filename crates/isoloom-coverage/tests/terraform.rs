//! The Terraform tables stay honest: every resource, data source, argument, nested block and
//! provider setting a generator writes for a committed example is a row marked as written, and
//! every row marked as written is written by some example. The tables themselves come from the
//! providers' schemas, so a written key with no row is an argument the provider doesn't have.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hcl::{Block, Body, Structure};
use isoloom_coverage::{Format, formats, written_problems};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Every Terraform module the generators write, under an example's `.isoloom/`: Proxmox,
/// docker-vm's Proxmox module, and each cloud's cloud-vm and cloud-docker modules.
const MODULES: &[&str] = &[
    "proxmox",
    "docker-vm/proxmox",
    "cloud-vm/aws",
    "cloud-vm/azure",
    "cloud-vm/gcp",
    "cloud-vm/digitalocean",
    "cloud-vm/linode",
    "cloud-vm/oci",
    "cloud-docker/aws",
    "cloud-docker/azure",
    "cloud-docker/gcp",
    "cloud-docker/digitalocean",
    "cloud-docker/linode",
    "cloud-docker/oci",
];

/// Terraform's own arguments, valid in every resource: not the provider's, so not rows.
const META: &[&str] = &["count", "for_each", "depends_on", "provider", "lifecycle", "provisioner", "connection"];

/// The `.tf` files of one module across the examples; each module must exist in some example.
fn module_files(module: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(examples()).unwrap() {
        let dir = entry.unwrap().path().join(".isoloom").join(module);
        let Ok(files) = std::fs::read_dir(&dir) else { continue };
        for f in files {
            let p = f.unwrap().path();
            if p.extension().is_some_and(|x| x == "tf") {
                found.push(std::fs::read_to_string(p).unwrap());
            }
        }
    }
    assert!(!found.is_empty(), "no example has a .isoloom/{module} module");
    found
}

/// The keys a body writes, under `prefix`: attributes, and nested blocks with their contents
/// (a `dynamic "x"` block counts as `x`, with its `content`).
fn walk(body: &Body, prefix: &str, top: bool, out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Structure::Attribute(a) if !(top && META.contains(&a.key.as_str())) => {
                out.insert(format!("{prefix}{}", a.key.as_str()));
            }
            Structure::Block(b) if !(top && META.contains(&b.identifier.as_str())) => nested(b, prefix, out),
            _ => {}
        }
    }
}

fn nested(b: &Block, prefix: &str, out: &mut BTreeSet<String>) {
    let (name, body) = if b.identifier.as_str() == "dynamic" {
        let content = b
            .body
            .blocks()
            .find(|c| c.identifier.as_str() == "content")
            .expect("a dynamic block has content");
        (b.labels[0].as_str().to_string(), &content.body)
    } else {
        (b.identifier.as_str().to_string(), &b.body)
    };
    out.insert(format!("{prefix}{name}"));
    walk(body, &format!("{prefix}{name}."), false, out);
}

/// Every key a module file writes, as the tables name them: `resource T`, `data T`,
/// `argument T.path`, `argument data.T.path` and `provider P: path`.
pub fn written_keys(text: &str) -> BTreeSet<String> {
    let body = hcl::parse(text).unwrap();
    let mut out = BTreeSet::new();
    for b in body.blocks() {
        let label = |i: usize| b.labels[i].as_str().to_string();
        match b.identifier.as_str() {
            "resource" => {
                out.insert(format!("resource {}", label(0)));
                walk(&b.body, &format!("argument {}.", label(0)), true, &mut out);
            }
            "data" => {
                out.insert(format!("data {}", label(0)));
                walk(&b.body, &format!("argument data.{}.", label(0)), true, &mut out);
            }
            "provider" => walk(&b.body, &format!("provider {}: ", label(0)), true, &mut out),
            _ => {}
        }
    }
    out
}

/// The Terraform table a key belongs to: the one with a row for its resource type, data
/// source or provider.
fn owner<'a>(tables: &'a [Format], key: &str) -> Option<&'a Format> {
    let anchor = if let Some(rest) = key.strip_prefix("argument data.") {
        format!("data {}", rest.split('.').next().unwrap())
    } else if let Some(rest) = key.strip_prefix("argument ") {
        format!("resource {}", rest.split('.').next().unwrap())
    } else if key.starts_with("provider ") {
        return tables
            .iter()
            .find(|f| f.rows.iter().any(|(k, _)| k.split(": ").next() == key.split(": ").next()));
    } else {
        key.to_string()
    };
    tables.iter().find(|f| f.rows.iter().any(|(k, _)| *k == anchor))
}

/// Every key written by any module, grouped by the table it belongs to.
fn written_by_table(tables: &[Format]) -> BTreeMap<&'static str, BTreeSet<String>> {
    let mut by: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    for module in MODULES {
        for text in module_files(module) {
            for key in written_keys(&text) {
                let table = owner(tables, &key).unwrap_or_else(|| panic!(".isoloom/{module}: writes `{key}`, which no Terraform table lists"));
                by.entry(table.name).or_default().insert(key);
            }
        }
    }
    by
}

fn terraform_tables() -> Vec<Format> {
    formats().into_iter().filter(|f| f.name.starts_with("Terraform: ")).collect()
}

#[test]
fn terraform_written_keys_match_the_generated_modules() {
    let tables = terraform_tables();
    let written = written_by_table(&tables);
    let problems: Vec<String> = tables
        .iter()
        .flat_map(|f| written_problems(f, &written.get(f.name).cloned().unwrap_or_default()))
        .collect();
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_classification_rule_names_something() {
    for t in isoloom_coverage::terraform::TABLES {
        let stale = t.stale_rules();
        assert!(stale.is_empty(), "{}: rules that name nothing in the provider's schema: {stale:?}", t.name);
    }
}

#[test]
#[ignore = "a tool: prints every key the modules write"]
fn print_written_keys() {
    let mut all = BTreeSet::new();
    for module in MODULES {
        for text in module_files(module) {
            all.extend(written_keys(&text));
        }
    }
    for k in all {
        println!("{k}");
    }
}
