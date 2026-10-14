//! The cloud-init, Ansible inventory and Dockerfile tables against their sources and against what
//! Isoloom writes: every row is an entry of the pinned list (and every entry a row), every key or
//! parameter a generator writes for an example is a row marked as written, and every row marked
//! as written is written for some example.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use hcl::{Body, Expression, ObjectKey, Structure, Template, TraversalOperator, template::Element};
use isoloom_coverage::{Format, cloud_init, dockerfile, inventory, written_problems};
use isoloom_plan::Environment;
use isoloom_render::inventory::{Access, render};
use isoloom_spec::model::Target;
use isoloom_spec::{Spec, load, parse, validate};
use isoloom_targets::{GENERATED_TARGETS, GeneratedFile, generate};

/// Fixtures for what no example exercises: a network's constraints (the cloud-init prelude) and
/// an existing Windows host (reached over SSH with PowerShell).
const CONSTRAINTS: &str = "
constraints:
  proxy: { http: \"http://proxy.corp:3128\" }
  mirrors: { debian: \"http://mirror.corp/debian\" }
  ntp: [ntp.corp]
";
const WINDOWS_EXTERNAL: &str = "
version: 1
name: windows-external
networks: { lab: { cidr: 192.168.1.0/24 } }
machines:
  dc:
    networks: { lab: 10 }
    vm: { os: windows-server-2022 }
    external: { address: 192.168.1.10, user: admin }
provision: [{ ansible: site.yml }]
";

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// The committed examples, then the fixtures.
fn specs() -> Vec<(String, Spec)> {
    let mut out = examples();
    let segmented = std::fs::read_to_string(examples_dir().join("segmented/isoloom.yml")).unwrap();
    out.push(("segmented with constraints".into(), parse(&format!("{segmented}{CONSTRAINTS}")).unwrap()));
    out.push(("windows-external".into(), parse(WINDOWS_EXTERNAL).unwrap()));
    for (name, spec) in &out[out.len() - 2..] {
        assert_eq!(validate(spec), vec![], "fixture {name}");
    }
    out
}

fn examples() -> Vec<(String, Spec)> {
    let dir = examples_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path: PathBuf = entry.unwrap().path();
        if path.join("isoloom.yml").exists() {
            out.push((path.file_name().unwrap().to_string_lossy().into_owned(), load(&path).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every file every generator writes for every example.
fn every_output() -> Vec<GeneratedFile> {
    let mut files = Vec::new();
    for (_, spec) in specs() {
        for target in GENERATED_TARGETS {
            files.extend(generate(&spec, *target).unwrap_or_default());
        }
    }
    files
}

/// A committed feature list: one entry a line, after its `#` header.
fn list(name: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("coverage").join(name)).unwrap();
    text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).map(str::to_string).collect()
}

fn rows_match_the_list(f: &Format, file: &str) {
    let expected = list(file);
    let rows: BTreeSet<String> = f.rows.iter().map(|(k, _)| k.clone()).collect();
    let missing: Vec<_> = expected.difference(&rows).collect();
    let extra: Vec<_> = rows.difference(&expected).collect();
    assert!(missing.is_empty(), "{}: entries of {file} with no row: {missing:?}", f.name);
    assert!(extra.is_empty(), "{}: rows that aren't in {file}: {extra:?}", f.name);
}

fn assert_truthful(f: &Format, written: &BTreeSet<String>) {
    let problems = written_problems(f, written);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_rows_are_exactly_the_pinned_lists() {
    rows_match_the_list(&cloud_init::format(), "cloud-init.txt");
    rows_match_the_list(&inventory::format(), "ansible-inventory.txt");
    rows_match_the_list(&dockerfile::format(), "dockerfile.txt");
}

// cloud-init: the documents are Terraform expressions (`format("#cloud-config\n%s",
// yamlencode({...}))`), read with hcl-rs, the locals they name followed.

/// Functions whose arguments are the document's own values (lists joined or filtered); any other
/// call makes a value of its own (a file's content), not cloud-config keys.
const PASSTHROUGH: &[&str] = &["concat", "compact", "flatten", "tolist"];

struct Walk<'a> {
    locals: &'a HashMap<String, Expression>,
    keys: BTreeSet<String>,
}

impl Walk<'_> {
    /// The keys of `e`, a value at `prefix` (`""`, `users.*.`).
    fn value(&mut self, e: &Expression, prefix: &str) {
        match e {
            Expression::Object(o) => {
                for (k, v) in o {
                    let name = match k {
                        ObjectKey::Identifier(i) => i.to_string(),
                        ObjectKey::Expression(Expression::String(s)) => s.clone(),
                        other => panic!("a computed cloud-config key: {other:?}"),
                    };
                    self.keys.insert(format!("{prefix}{name}"));
                    self.value(v, &format!("{prefix}{name}."));
                }
            }
            Expression::Array(items) => items.iter().for_each(|i| self.value(i, &format!("{prefix}*."))),
            Expression::ForExpr(f) => self.value(&f.value_expr, &format!("{prefix}*.")),
            Expression::Conditional(c) => {
                self.value(&c.true_expr, prefix);
                self.value(&c.false_expr, prefix);
            }
            Expression::Parenthesis(inner) => self.value(inner, prefix),
            Expression::FuncCall(call) if PASSTHROUGH.contains(&call.name.name.as_str()) => {
                call.args.iter().for_each(|a| self.value(a, prefix));
            }
            Expression::Traversal(t) => {
                if let (Expression::Variable(v), [TraversalOperator::GetAttr(name)]) = (&t.expr, t.operators.as_slice())
                    && v.as_str() == "local"
                {
                    let local = self.locals.get(name.as_str()).unwrap_or_else(|| panic!("no local `{name}`")).clone();
                    self.value(&local, prefix);
                }
            }
            _ => {}
        }
    }

    /// Finds the cloud-config documents in `e` and reads their keys.
    fn find(&mut self, e: &Expression) -> usize {
        match e {
            Expression::FuncCall(call) if call.name.name.as_str() == "format" => match call.args.as_slice() {
                [Expression::String(s), Expression::FuncCall(doc)] if s.starts_with("#cloud-config") => self.document(doc),
                args => args.iter().map(|a| self.find(a)).sum(),
            },
            Expression::TemplateExpr(t) => {
                let template = Template::from_expr(t).unwrap();
                match template.elements() {
                    [Element::Literal(s), Element::Interpolation(i), ..] if s.starts_with("#cloud-config") => match &i.expr {
                        Expression::FuncCall(doc) => self.document(doc),
                        other => panic!("a cloud-config document that isn't yamlencode: {other:?}"),
                    },
                    _ => 0,
                }
            }
            Expression::Object(o) => o.values().map(|v| self.find(v)).sum(),
            Expression::Array(items) => items.iter().map(|i| self.find(i)).sum(),
            Expression::FuncCall(call) => call.args.iter().map(|a| self.find(a)).sum(),
            Expression::Conditional(c) => self.find(&c.true_expr) + self.find(&c.false_expr),
            _ => 0,
        }
    }

    fn document(&mut self, doc: &hcl::FuncCall) -> usize {
        assert_eq!(doc.name.name.as_str(), "yamlencode", "a cloud-config document that isn't yamlencode");
        self.value(&doc.args[0], "");
        1
    }

    fn body(&mut self, body: &Body) -> usize {
        body.iter()
            .map(|s| match s {
                Structure::Attribute(a) => self.find(&a.expr),
                Structure::Block(b) => self.body(&b.body),
            })
            .sum()
    }
}

fn locals(body: &Body) -> HashMap<String, Expression> {
    body.blocks()
        .filter(|b| b.identifier() == "locals")
        .flat_map(|b| b.body.attributes())
        .map(|a| (a.key.to_string(), a.expr.clone()))
        .collect()
}

#[test]
fn cloud_init_written_keys_match_every_document_isoloom_writes() {
    let mut written = BTreeSet::new();
    let mut documents = 0;
    for file in every_output() {
        if !file.contents.contains("#cloud-config") {
            continue;
        }
        assert!(
            Path::new(&file.path).extension().is_some_and(|e| e == "tf"),
            "{}: a cloud-config document outside a Terraform module",
            file.path
        );
        let body: Body = hcl::parse(&file.contents).unwrap();
        let locals = locals(&body);
        let mut walk = Walk {
            locals: &locals,
            keys: BTreeSet::new(),
        };
        let found = walk.body(&body);
        assert!(found > 0, "{}: says #cloud-config, but no document was read", file.path);
        documents += found;
        written.extend(walk.keys);
    }
    assert!(documents > 0, "no example writes a cloud-config document");
    assert_truthful(&cloud_init::format(), &written);
}

// The inventory: rendered for every example in each target's access mode.

/// The `ansible_*` parameters of an inventory: on host lines and in `[group:vars]`.
fn parameters(inventory: &str) -> BTreeSet<String> {
    inventory
        .lines()
        .filter(|l| !l.starts_with('['))
        .flat_map(str::split_whitespace)
        .filter_map(|t| t.split_once('=').map(|(k, _)| k))
        .filter(|k| k.starts_with("ansible_"))
        .map(str::to_string)
        .collect()
}

/// How a target reaches its hosts: one of `Access`'s presets.
type AccessMode = fn() -> Access<'static>;

#[test]
fn inventory_written_parameters_match_every_access_mode() {
    let modes: [(&str, Target, AccessMode); 4] = [
        ("vagrant", Target::Vagrant, Access::vagrant),
        ("proxmox", Target::Proxmox, Access::proxmox),
        ("cloud", Target::CloudVm, || {
            Access::cloud(|_| "admin".to_string(), "${random_password.windows.result}")
        }),
        ("external", Target::External, Access::external),
    ];
    let mut written = BTreeSet::new();
    for (mode, target, access) in modes {
        let mut hosts = 0;
        for (name, spec) in specs() {
            if generate(&spec, target).is_err() {
                continue;
            }
            let env = Environment::build(&spec, target, None);
            let text = render(&env, &access());
            hosts += isoloom_render::inventory::hosts(&env).len();
            let found = parameters(&text);
            assert!(
                found.contains("ansible_host") || !text.contains(" ansible_host="),
                "{name} on {mode}: unread host lines"
            );
            written.extend(found);
        }
        assert!(hosts > 0, "no example has a host in the {mode} inventory");
    }
    assert_truthful(&inventory::format(), &written);
}

// Dockerfiles: every one Isoloom writes, read instruction by instruction.

/// The instructions, flags and parser directives of a Dockerfile.
fn dockerfile_entries(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut header = true;
    let mut continued = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(comment) = line.strip_prefix('#') {
            if let (true, Some((key, _))) = (header, comment.split_once('=')) {
                out.insert(format!("directive {}", key.trim().to_lowercase()));
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        header = false;
        if !continued {
            let mut words = line.split_whitespace();
            let instruction = words.next().unwrap().to_uppercase();
            for flag in words.take_while(|w| w.starts_with("--")) {
                out.insert(format!("{instruction} {}", flag.split('=').next().unwrap()));
            }
            out.insert(instruction);
        }
        continued = line.ends_with('\\');
    }
    out
}

#[test]
fn dockerfile_written_entries_match_every_dockerfile_isoloom_writes() {
    let mut written = BTreeSet::new();
    let mut found = 0;
    for file in every_output() {
        if file.path.rsplit('/').next().is_some_and(|n| n == "Dockerfile" || n.ends_with(".Dockerfile")) {
            found += 1;
            written.extend(dockerfile_entries(&file.contents));
        }
    }
    assert!(found > 0, "no example writes a Dockerfile");
    assert_truthful(&dockerfile::format(), &written);
}

#[test]
fn a_dockerfile_is_read_with_its_flags_and_directives() {
    let text = "# syntax=docker/dockerfile:1\n# a comment\nFROM --platform=linux/amd64 x\nRUN --mount=type=cache,target=/c a \\\n  --not-a-flag\n";
    let got: Vec<String> = dockerfile_entries(text).into_iter().collect();
    assert_eq!(got, ["FROM", "FROM --platform", "RUN", "RUN --mount", "directive syntax"]);
}
