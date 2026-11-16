//! `cargo xtask generate-all`: every example, or every spec folder given, generated for every
//! target it can be, into one tree that `cargo xtask verify` checks:
//!
//! - `<out>/<example>/`: a copy of the example's folder: its spec, and the files the generated
//!   ones refer to (build contexts, provision scripts), which the validators follow;
//! - `<out>/<example>/.isoloom/`: what `isoloom generate` writes, fresh (any outputs in the
//!   folder are left out of the copy);
//! - `<out>/generate.log`: what `isoloom generate` said, the targets it skipped included.
//!
//! The tree is rebuilt from scratch, so it holds only this run's files. A spec that fails to
//! generate stops the run. `ISOLOOM` names the binary; by default it is built with cargo.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::{Res, process, repo};

/// The arguments of `cargo xtask generate-all`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Where the tree goes (replaced).
    out: PathBuf,
    /// The spec folders (default: every example).
    specs: Vec<PathBuf>,
}

/// Builds the tree.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let isoloom = binary(&root)?;
    let specs = if a.specs.is_empty() { examples(&root)? } else { a.specs.clone() };
    generate(&isoloom, &specs, &a.out)?;
    Ok(ExitCode::SUCCESS)
}

/// The isoloom binary: `$ISOLOOM`, else built now (its path from cargo's own report).
pub(crate) fn binary(root: &Path) -> Res<PathBuf> {
    if let Some(bin) = std::env::var_os("ISOLOOM") {
        return Ok(PathBuf::from(bin));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(cargo);
    build.args(["build", "-q", "-p", "isoloom", "--message-format=json-render-diagnostics", "--manifest-path"]);
    build.arg(root.join("Cargo.toml")).stderr(Stdio::inherit());
    let report = process::output(&mut build)?;
    report
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|m| m.get("reason").and_then(serde_json::Value::as_str) == Some("compiler-artifact"))
        .filter(|m| m.get("target").and_then(|t| t.get("name")).and_then(serde_json::Value::as_str) == Some("isoloom"))
        .find_map(|m| m.get("executable").and_then(serde_json::Value::as_str).map(PathBuf::from))
        .ok_or_else(|| "cargo built no isoloom binary".into())
}

/// Every example folder, sorted.
pub(crate) fn examples(root: &Path) -> Res<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    Ok(out)
}

/// Copies each spec folder into `out` and generates it there.
pub(crate) fn generate(isoloom: &Path, specs: &[PathBuf], out: &Path) -> Res<()> {
    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)?;
    let log_path = out.join("generate.log");
    let mut log = OpenOptions::new().create(true).append(true).open(&log_path)?;
    for spec in specs {
        let name = spec.file_name().ok_or_else(|| format!("{}: not a folder", spec.display()))?;
        let dest = out.join(name);
        copy_tree(spec, &dest)?;
        std::io::Write::write_all(&mut log, format!("== {}\n", name.to_string_lossy()).as_bytes())?;
        let status = Command::new(isoloom)
            .arg("generate")
            .arg(&dest)
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?)
            .status()?;
        if !status.success() {
            return Err(format!("{} failed to generate (see {})", name.to_string_lossy(), log_path.display()).into());
        }
        if !dest.join(".isoloom").is_dir() {
            eprintln!("generate-all: {}: no target can be generated", name.to_string_lossy());
        }
    }
    eprintln!("generate-all: {} specs into {}", specs.len(), out.display());
    Ok(())
}

/// Whether a folder is a generated output (`.isoloom/`, `.isoloom-<n>/`), left out of copies.
fn is_output(name: &str) -> bool {
    name == ".isoloom" || name.starts_with(".isoloom-")
}

/// Copies `from` to `to`, symlinks as links, without the generated outputs at its top.
fn copy_tree(from: &Path, to: &Path) -> Res<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        if is_output(&name.to_string_lossy()) && from.join(&name).is_dir() {
            continue;
        }
        copy_entry(&entry.path(), &to.join(&name))?;
    }
    Ok(())
}

fn copy_entry(from: &Path, to: &Path) -> Res<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        copy_link(from, to)
    } else if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)?;
        Ok(())
    }
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> Res<()> {
    std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
    Ok(())
}

/// Windows: a link's target is copied (making links needs a privilege there).
#[cfg(not(unix))]
fn copy_link(from: &Path, to: &Path) -> Res<()> {
    if from.is_dir() {
        copy_tree(from, to)
    } else {
        std::fs::copy(from, to)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file and link under `dir`, by its path from `dir`, sorted; a link as `path -> target`.
    #[cfg(unix)]
    fn listing(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, at: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(at).unwrap().flatten() {
                let path = entry.path();
                let shown = path.strip_prefix(dir).unwrap().display().to_string();
                let kind = entry.file_type().unwrap();
                if kind.is_symlink() {
                    out.push(format!("{shown} -> {}", std::fs::read_link(&path).unwrap().display()));
                } else if kind.is_dir() {
                    walk(dir, &path, out);
                } else {
                    out.push(shown);
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    /// A stand-in `isoloom generate <folder>`: `bad` fails, `none` writes nothing, any other
    /// writes `.isoloom/docker/compose.yaml`; each says what it did.
    #[cfg(unix)]
    fn isoloom() -> isoloom_test_support::FakeBin {
        let bin = isoloom_test_support::FakeBin::named("xtask-generate-all").unwrap();
        bin.stand_in(
            "isoloom",
            r#"case "$2" in
  */bad) echo "bad: the spec doesn't parse"; exit 1 ;;
  */none) echo "no target can run it" ;;
  *) mkdir -p "$2/.isoloom/docker"; : > "$2/.isoloom/docker/compose.yaml"; echo "generated $(basename "$2")" ;;
esac"#,
        )
        .unwrap();
        bin
    }

    #[cfg(unix)]
    #[test]
    fn each_spec_is_copied_without_its_outputs_then_generated_into_a_fresh_tree() {
        let bin = isoloom();
        let specs = bin.dir().join("specs");
        let web = specs.join("web");
        for dir in ["build/app", ".isoloom/docker", ".isoloom-2"] {
            std::fs::create_dir_all(web.join(dir)).unwrap();
        }
        for file in [
            "isoloom.yml",
            "build/app/Dockerfile",
            ".isoloom/docker/stale.yaml",
            ".isoloom-2/old",
            ".isoloom-notes",
        ] {
            std::fs::write(web.join(file), "x").unwrap();
        }
        std::os::unix::fs::symlink("build/app", web.join("app")).unwrap();
        std::fs::create_dir_all(specs.join("none")).unwrap();
        let out = bin.dir().join("out");
        std::fs::create_dir_all(out.join("left-from-before")).unwrap();
        generate(&bin.path().join("isoloom"), &[web, specs.join("none")], &out).unwrap();
        assert_eq!(
            listing(&out),
            [
                "generate.log",
                "web/.isoloom-notes",
                "web/.isoloom/docker/compose.yaml",
                "web/app -> build/app",
                "web/build/app/Dockerfile",
                "web/isoloom.yml",
            ],
            "the outputs at the spec's top left out (a file named like one kept), a link kept a link, the tree made afresh"
        );
        let log = std::fs::read_to_string(out.join("generate.log")).unwrap();
        assert_eq!(log, "== web\ngenerated web\n== none\nno target can run it\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_spec_that_fails_to_generate_stops_the_run_naming_it_and_the_log() {
        let bin = isoloom();
        let specs = bin.dir().join("specs");
        for name in ["bad", "after"] {
            std::fs::create_dir_all(specs.join(name)).unwrap();
        }
        let out = bin.dir().join("out");
        let e = generate(&bin.path().join("isoloom"), &[specs.join("bad"), specs.join("after")], &out).unwrap_err();
        assert_eq!(e.to_string(), format!("bad failed to generate (see {})", out.join("generate.log").display()));
        assert_eq!(
            std::fs::read_to_string(out.join("generate.log")).unwrap(),
            "== bad\nbad: the spec doesn't parse\n"
        );
        assert!(!out.join("after").exists(), "the run stopped");
    }

    #[test]
    fn the_examples_are_the_folders_of_examples_sorted() {
        let root = tempfile::Builder::new().prefix("isoloom-xtask-examples-").tempdir().unwrap();
        for dir in ["examples/web", "examples/db"] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        std::fs::write(root.path().join("examples/README.md"), "").unwrap();
        let found = examples(root.path()).unwrap();
        assert_eq!(found, [root.path().join("examples/db"), root.path().join("examples/web")]);
    }
}
