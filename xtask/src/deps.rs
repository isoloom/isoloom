//! The one-way dependency rule of the engineering standard, checked on what
//! `cargo metadata` says: spec → plan → render → backends → targets → cli and the front-ends. A
//! crate depends only on crates below it, so no backend depends on another backend, the tools
//! (import) depend on the core alone, a front-end (the topology compiler, its WebAssembly engine)
//! builds on the targets and the front-ends before it, the manifest crate depends on no other crate of the workspace (it is the
//! serde-only contract the runner reads too, whose names the spec uses), and nothing depends on
//! the xtask (which may use
//! any crate: the catalogue's refresh reads the catalogue's types).
//!
//! The product boundary too: no crate of the runner (`isothrum`, `isothrum-*`) is anywhere in the
//! resolved graph, directly or transitively.

use serde_json::Value;

/// Where a crate sits: the lower the level, the fewer crates it may depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// A core crate, by its rank in the core order.
    Core(u8),
    /// A backend (`crates/backends/`).
    Backend,
    /// A tool (`crates/tools/`).
    Tool,
    /// The targets layer (`crates/targets/`).
    Targets,
    /// The command line (`crates/cli/`).
    Cli,
    /// A front-end (`crates/frontends/`), by its rank in the front-end order.
    Frontend(u8),
    /// The development tasks: on their own.
    Xtask,
}

impl Level {
    /// Its height: a crate may depend on a crate of a strictly lower height.
    fn height(self) -> u8 {
        match self {
            Level::Core(rank) => rank,
            Level::Backend | Level::Tool => 10,
            Level::Targets => 11,
            Level::Cli => 12,
            Level::Frontend(rank) => 12 + rank,
            Level::Xtask => 0,
        }
    }
}

/// The core crates, in their order; the manifest crate sits at the bottom, under the spec (which
/// reads the contract's names: its reserved prefix, the output folder).
const CORE: [(&str, u8); 5] = [
    ("isoloom-manifest", 0),
    ("isoloom-spec", 1),
    ("isoloom-catalogue", 2),
    ("isoloom-plan", 3),
    ("isoloom-render", 4),
];

/// The front-ends, in their order: the WebAssembly engine runs the topology compiler.
const FRONTENDS: [(&str, u8); 2] = [("isoloom-topology", 0), ("isoloom-wasm", 1)];

/// What the manifest crate may build on: serde and its schema, nothing else, so the runner reads
/// the contract without pulling in the converter.
const MANIFEST_DEPS: [&str; 5] = ["serde", "serde_json", "serde_path_to_error", "schemars", "thiserror"];

/// The manifest crate's dependencies beyond [`MANIFEST_DEPS`] (its tests' aside).
fn manifest_extras(package: &Value) -> Vec<String> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["kind"].is_null())
        .filter_map(|d| d["name"].as_str())
        .filter(|n| !MANIFEST_DEPS.contains(n))
        .map(|n| {
            format!(
                "isoloom-manifest depends on {n}: the manifest crate stays serde-only ({})",
                MANIFEST_DEPS.join(", ")
            )
        })
        .collect()
}

/// The level of the crate `name` whose folder is `dir` (relative to the workspace, with `/`).
fn level(name: &str, dir: &str) -> Result<Level, String> {
    let group = dir.split('/').collect::<Vec<_>>();
    match group.as_slice() {
        ["crates", "core", _] => CORE
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, rank)| Level::Core(*rank))
            .ok_or_else(|| format!("{name}: a core crate the dependency rule doesn't place: add it to the core order (xtask/src/deps.rs)")),
        ["crates", "backends", _] => Ok(Level::Backend),
        ["crates", "tools", _] => Ok(Level::Tool),
        ["crates", "targets", _] => Ok(Level::Targets),
        ["crates", "cli", _] => Ok(Level::Cli),
        ["crates", "frontends", _] => FRONTENDS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, rank)| Level::Frontend(*rank))
            .ok_or_else(|| format!("{name}: a front-end the dependency rule doesn't place: add it to the front-end order (xtask/src/deps.rs)")),
        ["xtask"] => Ok(Level::Xtask),
        _ => Err(format!(
            "{name} ({dir}): outside the layout (crates/{{core,backends,tools,targets,cli,frontends}}/<crate>, xtask)"
        )),
    }
}

/// The crate of helpers the tests share (unpublished, on no workspace crate): a dev-dependency
/// any crate may take.
const TEST_SUPPORT: &str = "isoloom-test-support";

/// Whether `from` may depend on `to`.
fn allowed(from: Level, to: Level) -> bool {
    match (from, to) {
        (_, Level::Xtask) | (Level::Backend | Level::Tool, Level::Backend | Level::Tool) => false,
        (Level::Xtask, _) => true,
        _ => to.height() < from.height(),
    }
}

/// Every edge between workspace crates that breaks the rule, and every crate the rule can't
/// place, from `cargo metadata --no-deps` output.
pub(crate) fn violations(metadata: &Value) -> Vec<String> {
    let root = metadata["workspace_root"].as_str().unwrap_or("").replace('\\', "/");
    let packages = metadata["packages"].as_array().cloned().unwrap_or_default();
    let mut problems = Vec::new();
    let mut levels = Vec::new();
    for p in &packages {
        let name = p["name"].as_str().unwrap_or("");
        let manifest = p["manifest_path"].as_str().unwrap_or("").replace('\\', "/");
        let dir = manifest.trim_end_matches("/Cargo.toml").trim_start_matches(&root).trim_start_matches('/');
        match level(name, dir) {
            Ok(l) => levels.push((name.to_owned(), l)),
            Err(e) => problems.push(e),
        }
    }
    let level_of = |n: &str| levels.iter().find(|(name, _)| name == n).map(|(_, l)| *l);
    for p in &packages {
        let name = p["name"].as_str().unwrap_or("");
        if name == "isoloom-manifest" {
            problems.extend(manifest_extras(p));
        }
        let Some(from) = level_of(name) else { continue };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or("");
            let Some(to) = level_of(dep) else { continue };
            // The tests' shared helpers sit beneath every crate: any crate's tests may use them.
            if dep == TEST_SUPPORT && d["kind"].as_str() == Some("dev") {
                continue;
            }
            if !allowed(from, to) {
                let kind = d["kind"].as_str().map(|k| format!(" ({k})")).unwrap_or_default();
                problems.push(format!(
                    "{name} ({from:?}) depends on {dep} ({to:?}){kind}: dependencies go one way, spec → plan → render → backends → targets → cli and the front-ends"
                ));
            }
        }
    }
    problems
}

/// The other product, the runner: Isoloom converts specs into files and never builds on it, not
/// even through another crate (it is Isothrum that reads Isoloom's output).
const RUNNER_PREFIX: &str = "isothrum";

/// Every crate of the runner (`isothrum`, `isothrum-*`) in the resolved dependency graph of
/// `cargo metadata` (with its dependencies, so the transitive ones count), each with the crates
/// that pull it in.
pub(crate) fn runner_crates(metadata: &Value) -> Vec<String> {
    let id_name = |id: &str| {
        metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"].as_str() == Some(id))
            .and_then(|p| p["name"].as_str())
            .unwrap_or(id)
            .to_owned()
    };
    let nodes = metadata["resolve"]["nodes"].as_array().cloned().unwrap_or_default();
    let mut found = Vec::new();
    for node in &nodes {
        let id = node["id"].as_str().unwrap_or("");
        let name = id_name(id);
        if name != RUNNER_PREFIX && !name.starts_with(&format!("{RUNNER_PREFIX}-")) {
            continue;
        }
        let by: Vec<String> = nodes
            .iter()
            .filter(|n| n["dependencies"].as_array().into_iter().flatten().any(|d| d.as_str() == Some(id)))
            .map(|n| id_name(n["id"].as_str().unwrap_or("")))
            .collect();
        found.push(format!(
            "{name} is in Isoloom's dependency graph (pulled in by {}): Isoloom never depends on the runner",
            if by.is_empty() { "nothing named".to_owned() } else { by.join(", ") }
        ));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, dir: &str, deps: &[(&str, Option<&str>)]) -> Value {
        let deps: Vec<Value> = deps.iter().map(|(n, k)| serde_json::json!({ "name": n, "kind": k })).collect();
        serde_json::json!({ "name": name, "manifest_path": format!("/ws/{dir}/Cargo.toml"), "dependencies": deps })
    }

    fn check(packages: &[Value]) -> Vec<String> {
        violations(&serde_json::json!({ "workspace_root": "/ws", "packages": packages }))
    }

    #[test]
    fn the_layers_may_depend_downwards_only() {
        let ok = check(&[
            package("isoloom-spec", "crates/core/isoloom-spec", &[]),
            package("isoloom-plan", "crates/core/isoloom-plan", &[("isoloom-spec", None)]),
            package("isoloom-compose", "crates/backends/isoloom-compose", &[("isoloom-plan", None)]),
            package(
                "isoloom-targets",
                "crates/targets/isoloom-targets",
                &[("isoloom-compose", None), ("isoloom-import", Some("dev"))],
            ),
            package("isoloom-import", "crates/tools/isoloom-import", &[("isoloom-spec", None)]),
        ]);
        assert!(ok.is_empty(), "{ok:?}");
        let upward = check(&[
            package("isoloom-spec", "crates/core/isoloom-spec", &[("isoloom-plan", Some("dev"))]),
            package("isoloom-plan", "crates/core/isoloom-plan", &[]),
        ]);
        assert_eq!(upward.len(), 1, "{upward:?}");
        assert!(
            upward[0].starts_with("isoloom-spec (Core(1)) depends on isoloom-plan (Core(3)) (dev)"),
            "{upward:?}"
        );
    }

    #[test]
    fn any_crate_s_tests_may_use_the_shared_test_helpers_and_nothing_else_may() {
        let helpers = || package(TEST_SUPPORT, "crates/tools/isoloom-test-support", &[]);
        let ok = check(&[
            helpers(),
            package("isoloom-vagrant", "crates/backends/isoloom-vagrant", &[(TEST_SUPPORT, Some("dev"))]),
        ]);
        assert!(ok.is_empty(), "{ok:?}");
        let built = check(&[
            helpers(),
            package("isoloom-vagrant", "crates/backends/isoloom-vagrant", &[(TEST_SUPPORT, None)]),
        ]);
        assert_eq!(built.len(), 1, "{built:?}");
    }

    #[test]
    fn a_front_end_builds_on_the_targets_and_the_front_ends_before_it() {
        let ok = check(&[
            package("isoloom-targets", "crates/targets/isoloom-targets", &[]),
            package("isoloom-topology", "crates/frontends/isoloom-topology", &[("isoloom-targets", Some("dev"))]),
            package(
                "isoloom-wasm",
                "crates/frontends/isoloom-wasm",
                &[("isoloom-targets", None), ("isoloom-topology", None)],
            ),
        ]);
        assert!(ok.is_empty(), "{ok:?}");
        let found = check(&[
            package("isoloom-topology", "crates/frontends/isoloom-topology", &[("isoloom-wasm", None)]),
            package("isoloom-wasm", "crates/frontends/isoloom-wasm", &[]),
            package("isoloom", "crates/cli/isoloom", &[("isoloom-topology", None)]),
        ]);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn a_backend_never_depends_on_another_nor_a_tool_on_a_backend() {
        let found = check(&[
            package("isoloom-compose", "crates/backends/isoloom-compose", &[]),
            package("isoloom-vagrant", "crates/backends/isoloom-vagrant", &[("isoloom-compose", None)]),
            package("isoloom-import", "crates/tools/isoloom-import", &[("isoloom-compose", None)]),
        ]);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn the_manifest_stands_alone_and_nothing_uses_the_xtask() {
        let found = check(&[
            package("isoloom-spec", "crates/core/isoloom-spec", &[]),
            package(
                "isoloom-manifest",
                "crates/core/isoloom-manifest",
                &[("isoloom-spec", None), ("serde", None), ("proptest", Some("dev"))],
            ),
            package("isoloom", "crates/cli/isoloom", &[("xtask", Some("dev"))]),
            // The xtask may use any crate (the catalogue's refresh reads its types).
            package("xtask", "xtask", &[("isoloom-spec", None)]),
        ]);
        // isoloom-spec twice: a workspace crate, and not serde.
        assert_eq!(found.len(), 3, "{found:?}");
    }

    #[test]
    fn a_crate_outside_the_layout_or_the_core_order_is_named() {
        let found = check(&[package("isoloom-new", "crates/core/isoloom-new", &[]), package("stray", "stray", &[])]);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains("add it to the core order"), "{found:?}");
    }

    /// The workspace itself, as `cargo metadata` describes it.
    #[test]
    fn the_workspace_keeps_its_dependencies_one_way() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let out = std::process::Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path"])
            .arg(manifest)
            .output()
            .expect("cargo metadata runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let metadata: Value = serde_json::from_slice(&out.stdout).expect("cargo metadata prints JSON");
        assert!(metadata["packages"].as_array().is_some_and(|p| p.len() > 10), "the workspace's crates");
        let found = violations(&metadata);
        assert!(found.is_empty(), "{}", found.join("\n"));
    }

    fn resolved(packages: &[(&str, &[&str])]) -> Value {
        let pkgs: Vec<Value> = packages
            .iter()
            .map(|(n, _)| serde_json::json!({ "id": format!("{n} 1.0.0"), "name": n }))
            .collect();
        let nodes: Vec<Value> = packages
            .iter()
            .map(|(n, deps)| {
                let deps: Vec<String> = deps.iter().map(|d| format!("{d} 1.0.0")).collect();
                serde_json::json!({ "id": format!("{n} 1.0.0"), "dependencies": deps })
            })
            .collect();
        serde_json::json!({ "packages": pkgs, "resolve": { "nodes": nodes } })
    }

    #[test]
    fn a_runner_crate_anywhere_in_the_graph_is_named_with_who_pulls_it_in() {
        let clean = resolved(&[("isoloom", &["isoloom-spec"]), ("isoloom-spec", &["serde"]), ("serde", &[])]);
        assert_eq!(runner_crates(&clean), Vec::<String>::new());
        // Transitive: isoloom → some-lib → isothrum-core.
        let pulled = resolved(&[
            ("isoloom", &["some-lib"]),
            ("some-lib", &["isothrum-core"]),
            ("isothrum-core", &[]),
            ("isothrum", &[]),
        ]);
        let found = runner_crates(&pulled);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("isothrum-core is in") && found[0].contains("some-lib"), "{found:?}");
        // A name that only starts like it is not the runner.
        assert_eq!(runner_crates(&resolved(&[("isothrumbus", &[])])), Vec::<String>::new());
    }

    /// The product boundary: nothing in Isoloom's resolved graph is the runner's, not even
    /// transitively.
    #[test]
    fn isoloom_never_depends_on_the_runner() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let out = std::process::Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--offline", "--manifest-path"])
            .arg(manifest)
            .output()
            .expect("cargo metadata runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let metadata: Value = serde_json::from_slice(&out.stdout).expect("cargo metadata prints JSON");
        assert!(metadata["resolve"]["nodes"].as_array().is_some_and(|n| n.len() > 50), "the resolved graph");
        let found = runner_crates(&metadata);
        assert!(found.is_empty(), "{}", found.join("\n"));
    }
}
