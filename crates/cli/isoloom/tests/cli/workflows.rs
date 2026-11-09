//! The CI workflow runs what exists: every `cargo` command names a package of the workspace
//! (`-p`), a test target it names (`--test`) is one of that package's, every `isoloom`
//! subcommand it calls is one the command line has, a crate list it compares is the
//! workspace's, no generation is let fail quietly, every document the workflows point to is
//! there, and the Isothrum runner is built only at a pinned ref.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::testing::{Outcome, isoloom, scratch};

/// The repository's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The workspace's packages: each one's test targets, and its manifest relative to the root.
struct Workspace {
    tests: BTreeMap<String, BTreeSet<String>>,
    manifests: BTreeMap<String, String>,
}

/// The workspace, from `cargo metadata`.
fn workspace() -> Outcome<Workspace> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root())
        .output()?;
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let root_dir = meta.get("workspace_root").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned();
    let (mut tests, mut manifests) = (BTreeMap::new(), BTreeMap::new());
    for p in meta.get("packages").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or_default().to_owned();
        let targets = p["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["kind"].as_array().is_some_and(|k| k.iter().any(|k| k == "test")))
            .filter_map(|t| t["name"].as_str().map(str::to_owned))
            .collect();
        let manifest = p["manifest_path"].as_str().unwrap_or_default();
        let relative = manifest
            .strip_prefix(&root_dir)
            .unwrap_or(manifest)
            .trim_start_matches(['/', '\\'])
            .replace('\\', "/");
        tests.insert(name.clone(), targets);
        manifests.insert(name, relative);
    }
    Ok(Workspace { tests, manifests })
}

/// The command lines of `ci.yml`: every line but comments, each as its words.
fn lines() -> Outcome<Vec<Vec<String>>> {
    let text = std::fs::read_to_string(root().join(".github/workflows/ci.yml"))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            l.trim_start_matches("- ")
                .trim_start_matches("run:")
                .split_whitespace()
                .map(str::to_owned)
                .collect()
        })
        .collect())
}

/// The value after `flag` in `words`.
fn after<'a>(words: &'a [String], flag: &str) -> Option<&'a str> {
    words.iter().position(|w| w == flag).and_then(|i| words.get(i + 1)).map(String::as_str)
}

#[test]
fn every_cargo_command_names_a_package_and_a_test_target_it_has() -> Outcome {
    let ws = workspace()?;
    for words in lines()? {
        let Some(at) = words.iter().position(|w| w == "cargo") else { continue };
        let words = words.get(at..).unwrap_or_default();
        let sub = words.get(1).map(String::as_str).unwrap_or_default();
        if !["test", "run", "build"].contains(&sub) || words.iter().any(|w| w == "--manifest-path") {
            continue;
        }
        let package = after(words, "-p");
        if let Some(test) = after(words, "--test") {
            let package = package.ok_or_else(|| format!("`--test {test}` without `-p`: {words:?}"))?;
            let tests = ws.tests.get(package).ok_or_else(|| format!("no package `{package}`: {words:?}"))?;
            assert!(tests.contains(test), "`{package}` has no test target `{test}` (it has {tests:?}): {words:?}");
        } else if let Some(package) = package {
            assert!(ws.tests.contains_key(package), "no package `{package}`: {words:?}");
        }
    }
    Ok(())
}

#[test]
fn every_isoloom_subcommand_it_calls_exists() -> Outcome {
    let home = scratch("workflow-subcommands")?;
    let mut called = BTreeSet::new();
    for words in lines()? {
        for (i, w) in words.iter().enumerate() {
            let next = |n: usize| words.get(i + n).map(String::as_str);
            let sub = if w == "isoloom" && i > 0 && words.get(i - 1).is_some_and(|p| p == "-p") {
                (next(1) == Some("--")).then(|| next(2)).flatten()
            } else if (w == "isoloom" || w.ends_with("/isoloom")) && !(i > 0 && words.get(i - 1).is_some_and(|p| p.starts_with('-'))) {
                // A program, not a flag's value (`--name isoloom`).
                next(1)
            } else {
                None
            };
            called.extend(
                sub.filter(|s| s.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !s.starts_with('-'))
                    .map(str::to_owned),
            );
        }
    }
    assert!(called.contains("generate"), "{called:?}");
    for sub in called {
        let run = isoloom(&home, &[&sub.as_str(), &"--help"])?;
        assert_eq!(run.code, Some(0), "`isoloom {sub}` doesn't exist: {run:?}");
    }
    Ok(())
}

#[test]
fn the_api_jobs_crates_are_the_workspaces_in_their_group_folders() -> Outcome {
    let ws = workspace()?;
    let text = std::fs::read_to_string(root().join(".github/workflows/ci.yml"))?;
    assert!(
        !text.contains("crates/$1/"),
        "crates sit in group folders (crates/core/, ...), not under crates/ directly"
    );
    let lists: Vec<&str> = text.lines().filter_map(|l| l.trim().strip_prefix("for crate in ")).collect();
    assert_ne!(lists, Vec::<&str>::new(), "the API job lists its crates");
    for list in lists {
        for name in list.trim_end_matches("; do").split_whitespace() {
            let manifest = ws.manifests.get(name).ok_or_else(|| format!("no crate `{name}` in the workspace"))?;
            let parts: Vec<&str> = manifest.split('/').collect();
            assert!(
                matches!(parts.as_slice(), ["crates", _, crate_dir, "Cargo.toml"] if *crate_dir == name),
                "`{name}` is at {manifest}, not crates/<group>/{name}/Cargo.toml"
            );
        }
    }
    Ok(())
}

#[test]
fn no_generation_is_let_fail_quietly() -> Outcome {
    for words in lines()? {
        let generates = words
            .windows(2)
            .any(|w| (w[0] == "isoloom" || w[0].ends_with("/isoloom") || w[0] == "--") && w[1] == "generate");
        let tolerated = words.windows(2).any(|w| w[0] == "||" && w[1] == "true");
        assert!(!(generates && tolerated), "a failing generate must fail the job: {words:?}");
    }
    Ok(())
}

#[test]
fn ci_runs_every_gate_the_release_gate_runs() -> Outcome {
    // CONTRIBUTING: every gate runs locally and in CI. gate.yml (the release path) is the list;
    // ci.yml's `check` job runs each of its commands too (`--locked` aside).
    let commands = |file: &str| -> Outcome<Vec<String>> {
        let text = std::fs::read_to_string(root().join(".github/workflows").join(file))?;
        Ok(text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("- run: "))
            .map(|c| c.replace(" --locked", ""))
            .collect())
    };
    let ci = commands("ci.yml")?;
    let gate = commands("gate.yml")?;
    assert!(gate.iter().any(|c| c.contains("xtask -- quality")), "{gate:?}");
    for c in gate {
        assert!(ci.contains(&c), "ci.yml doesn't run the gate `{c}`");
    }
    Ok(())
}

/// The repository's Markdown files a workflow points its reader to (`docs/dev/releasing.md`,
/// `SECURITY.md`): each word ending in `.md` in its comments and in the notices it prints, from
/// the root. Its commands' own files (release notes it writes) aren't pointers.
fn named_documents(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('#') || l.contains("::notice") || l.contains("::warning"))
        .flat_map(|l| l.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ';' | ',' | '"' | '`')))
        .map(|w| w.trim_end_matches(['.', ':']))
        .filter(|w| Path::new(w).extension().is_some_and(|e| e == "md") && !w.contains("://"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_document_the_workflows_point_to_is_there() -> Outcome {
    let github = root().join(".github");
    let mut files: Vec<PathBuf> = std::fs::read_dir(github.join("workflows"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    files.push(github.join("actionlint.yaml"));
    let mut missing = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)?;
        // What the workflow writes itself (`-o THIRD-PARTY-LICENSES.md`) is no pointer.
        let written = |doc: &str| text.contains(&format!("-o {doc}")) || text.contains(&format!("> {doc}"));
        for doc in named_documents(&text) {
            if !written(&doc) && !root().join(&doc).is_file() {
                missing.push(format!("{}: {doc}", file.display()));
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
    Ok(())
}

/// What `.github/scripts/isothrum-ref.sh <mode>` says of `reference`: its exit code.
#[cfg(unix)]
fn pinned(mode: &str, reference: &str) -> Outcome<Option<i32>> {
    let script = root().join(".github/scripts/isothrum-ref.sh");
    Ok(Command::new("bash")
        .arg(script)
        .arg(mode)
        .env("ISOTHRUM_REF", reference)
        .output()?
        .status
        .code())
}

#[cfg(unix)]
#[test]
fn the_isothrum_runner_is_built_only_at_a_release_tag_or_a_commit() -> Outcome {
    for reference in ["v2.0.0", "v2.0.0-rc.1", "0123456789abcdef0123456789abcdef01234567"] {
        assert_eq!(pinned("required", reference)?, Some(0), "{reference}");
    }
    for reference in ["main", "release/2.0", "0123abc"] {
        assert_eq!(pinned("optional", reference)?, Some(1), "{reference}");
    }
    assert_eq!(pinned("optional", "")?, Some(3), "unset: the runs that need it are skipped");
    assert_eq!(pinned("required", "")?, Some(1));
    // Every workflow that builds the runner checks its ref first, and none falls back to a branch.
    for entry in std::fs::read_dir(root().join(".github/workflows"))? {
        let text = std::fs::read_to_string(entry?.path())?;
        if let Some(checkout) = text.find("repository: Isothrum/isothrum") {
            let check = text.find(".github/scripts/isothrum-ref.sh").ok_or("no ref check")?;
            assert!(check < checkout && !text.contains("ISOTHRUM_REF ||"), "{text}");
        }
    }
    Ok(())
}
