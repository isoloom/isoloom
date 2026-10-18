//! The one-way dependency rule of the engineering standard (PLAN 3b), checked on what
//! `cargo metadata` says: spec → plan → render → backends → targets → cli. A crate depends only
//! on crates below it, so no backend depends on another backend, the tools (import) depend on
//! the core alone, the manifest crate depends on no other crate of the workspace (it is the
//! serde-only contract the runner reads too), and nothing depends on the xtask (which may use
//! any crate: the catalogue's refresh reads the catalogue's types).

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
            Level::Xtask => 0,
        }
    }
}

/// The core crates, in their order; the manifest crate sits at the bottom with the spec.
const CORE: [(&str, u8); 5] = [
    ("isoloom-spec", 0),
    ("isoloom-manifest", 0),
    ("isoloom-catalogue", 1),
    ("isoloom-plan", 2),
    ("isoloom-render", 3),
];

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
            .ok_or_else(|| format!("{name}: a core crate the dependency rule doesn't place yet: add it to the core order (xtask/src/deps.rs)")),
        ["crates", "backends", _] => Ok(Level::Backend),
        ["crates", "tools", _] => Ok(Level::Tool),
        ["crates", "targets", _] => Ok(Level::Targets),
        ["crates", "cli", _] => Ok(Level::Cli),
        ["xtask"] => Ok(Level::Xtask),
        _ => Err(format!(
            "{name} ({dir}): outside the layout (crates/{{core,backends,tools,targets,cli}}/<crate>, xtask)"
        )),
    }
}

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
            if !allowed(from, to) {
                let kind = d["kind"].as_str().map(|k| format!(" ({k})")).unwrap_or_default();
                problems.push(format!(
                    "{name} ({from:?}) depends on {dep} ({to:?}){kind}: dependencies go one way, spec → plan → render → backends → targets → cli"
                ));
            }
        }
    }
    problems
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
            upward[0].starts_with("isoloom-spec (Core(0)) depends on isoloom-plan (Core(2)) (dev)"),
            "{upward:?}"
        );
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
}
