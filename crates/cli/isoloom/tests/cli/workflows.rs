//! The CI workflow runs what exists: every `cargo` command names a package of the workspace
//! (`-p`), a test target it names (`--test`) is one of that package's, every `isoloom`
//! subcommand it calls is one the command line has, a crate list it compares is the
//! workspace's, no generation is let fail quietly, every document the workflows point to is
//! there, the Isothrum runner is built only at a pinned ref, and the Windows binary's gate
//! refuses one that needs the Visual C++ runtime.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::testing::{Outcome, isoloom, scratch};

/// The repository's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The workspace's packages: each one's test targets.
struct Workspace {
    tests: BTreeMap<String, BTreeSet<String>>,
}

/// The workspace, from `cargo metadata`.
fn workspace() -> Outcome<Workspace> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root())
        .output()?;
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let mut tests = BTreeMap::new();
    for p in meta.get("packages").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or_default().to_owned();
        let targets = p["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["kind"].as_array().is_some_and(|k| k.iter().any(|k| k == "test")))
            .filter_map(|t| t["name"].as_str().map(str::to_owned))
            .collect();
        tests.insert(name, targets);
    }
    Ok(Workspace { tests })
}

/// The command lines of every workflow: every line but comments, each as its words.
fn lines() -> Outcome<Vec<Vec<String>>> {
    let mut text = String::new();
    for entry in std::fs::read_dir(root().join(".github/workflows"))? {
        text.push_str(&std::fs::read_to_string(entry?.path())?);
        text.push('\n');
    }
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
#[ignore = "needs cargo: reads the workspace with `cargo metadata` (a player's machine has no toolchain); `cargo xtask test` runs it"]
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
    for sub in called {
        let run = isoloom(&home, &[&sub.as_str(), &"--help"])?;
        assert_eq!(run.code, Some(0), "`isoloom {sub}` doesn't exist: {run:?}");
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

/// The repository's Markdown files a workflow points its reader to (`.github/CONTRIBUTING.md`,
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
    let files: Vec<PathBuf> = std::fs::read_dir(github.join("workflows"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
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

/// Isoloom's output is validated, never run: no workflow builds a runner or starts what a target
/// generates, and CI checks every target's files with their validators (`cargo xtask verify`).
#[test]
fn the_workflows_validate_the_generated_files_and_never_run_them() -> Outcome {
    let starts = [
        ["isothrum", "up"],
        ["compose", "up"],
        ["kubectl", "apply"],
        ["helm", "install"],
        ["vagrant", "up"],
        ["terraform", "apply"],
    ];
    for entry in std::fs::read_dir(root().join(".github/workflows"))? {
        let path = entry?.path();
        let text = std::fs::read_to_string(&path)?;
        assert!(!text.contains("repository: Isothrum/isothrum"), "{} builds the runner", path.display());
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let words: Vec<&str> = line.split_whitespace().collect();
            for start in &starts {
                assert!(
                    !words.windows(2).any(|w| w == start.as_slice()),
                    "{} runs generated files (`{}`): {line}",
                    path.display(),
                    start.join(" ")
                );
            }
        }
    }
    Ok(())
}

/// What `.github/scripts/no-vc-runtime.sh` says of an executable whose imports (an
/// `llvm-objdump -p` standing in, listing `dlls`) are those: its exit code and what it printed.
#[cfg(unix)]
fn imports(dlls: &[&str]) -> Outcome<(Option<i32>, String)> {
    let bin = isoloom_test_support::FakeBin::named("no-vc-runtime")?;
    let mut script = String::from("echo 'isoloom.exe: file format coff-x86-64'\n");
    for dll in dlls {
        script.push_str("echo '    DLL Name: ");
        script.push_str(dll);
        script.push_str("'\n");
    }
    bin.stand_in("llvm-objdump", &script)?;
    let out = Command::new("bash")
        .arg(root().join(".github/scripts/no-vc-runtime.sh"))
        .arg(bin.path().join("llvm-objdump"))
        .arg("isoloom.exe")
        .output()?;
    Ok((out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// v2.1.1's isoloom.exe imported VCRUNTIME140.dll, and a Windows without the redistributable
/// couldn't start it: the release's gate refuses that, and passes one with the C runtime linked in.
#[cfg(unix)]
#[test]
fn the_windows_binary_gate_refuses_one_that_needs_the_visual_cpp_runtime() -> Outcome {
    let windows = ["kernel32.dll", "ntdll.dll", "bcryptprimitives.dll", "api-ms-win-core-synch-l1-2-0.dll"];
    for runtime in ["VCRUNTIME140.dll", "vcruntime140_1.dll", "MSVCP140.dll"] {
        let (code, said) = imports(&[&windows[..], &[runtime, "api-ms-win-crt-heap-l1-1-0.dll"]].concat())?;
        assert_eq!(code, Some(1), "{runtime}: {said}");
        assert!(said.contains(&format!("imports {runtime}")), "{said}");
    }
    assert_eq!(imports(&windows)?.0, Some(0), "the C runtime linked in");
    Ok(())
}
