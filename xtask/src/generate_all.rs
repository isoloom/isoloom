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

    #[test]
    fn only_isoloom_outputs_are_left_out() {
        assert!(is_output(".isoloom") && is_output(".isoloom-2"));
        assert!(!is_output("isoloom.yml") && !is_output(".isoloomignore"));
    }
}
