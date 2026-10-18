//! What the sources themselves say: the workspace's crates (from `cargo metadata`), the files
//! over the size limit, and the exceptions (`#[expect]`).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Counts, Crate, add, path_in};
use crate::{Res, process, repo};

/// Lines of code a source file may have.
const MAX_CODE_LINES: usize = 400;

/// The workspace's crates, from `cargo metadata`.
pub(crate) fn crates(root: &Path) -> Res<Vec<Crate>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args(["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"));
    let metadata: serde_json::Value = serde_json::from_str(&process::output(&mut cmd)?)?;
    let base = repo::slashed(Path::new(metadata.get("workspace_root").and_then(serde_json::Value::as_str).unwrap_or("")));
    let packages = metadata.get("packages").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    Ok(packages
        .iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?.to_owned();
            let manifest = repo::slashed(Path::new(p.get("manifest_path")?.as_str()?));
            let dir = manifest.strip_suffix("/Cargo.toml")?.strip_prefix(&base)?.trim_start_matches('/').to_owned();
            Some(Crate { name, dir })
        })
        .collect())
}

/// Counts the files over the size limit (a crate's `src`, its `tests.rs` files aside) and the
/// `#[expect]` exceptions (anywhere in the crate).
pub(crate) fn measure(root: &Path, crates: &[Crate], counts: &mut Counts) -> Res<()> {
    for c in crates {
        let dir = path_in(root, &c.dir);
        for file in rust_files(&dir.join("src")) {
            let text = std::fs::read_to_string(&file)?;
            if file.file_name().is_some_and(|n| n != "tests.rs") && code_lines(&text) > MAX_CODE_LINES {
                add(counts, "file/over-400-loc", &c.name);
            }
        }
        for file in rust_files(&dir) {
            let text = std::fs::read_to_string(&file)?;
            for _ in 0..expectations(&text) {
                add(counts, "exception/expect", &c.name);
            }
        }
    }
    Ok(())
}

/// Every `.rs` file under `dir`, recursively, build folders aside.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                out.extend(rust_files(&path));
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
    out
}

/// The lines of `src` that count: not blank, not a comment, not inside the inline test module.
/// rustfmt closes a top-level module with a `}` alone at column 0, which ends the test block.
pub(crate) fn code_lines(src: &str) -> usize {
    let mut count = 0;
    let mut in_tests = false;
    let mut previous = "";
    for line in src.lines() {
        let trimmed = line.trim();
        if in_tests {
            in_tests = line != "}";
        } else if previous == "#[cfg(test)]" && trimmed.starts_with("mod tests") && trimmed.ends_with('{') {
            in_tests = true;
            count -= 1;
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            count += 1;
        }
        previous = trimmed;
    }
    count
}

/// How many `#[expect(` and `#![expect(` attributes `src` holds.
fn expectations(src: &str) -> usize {
    src.lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("#[expect(") || l.starts_with("#![expect("))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_blanks_and_the_test_module_are_not_counted() {
        let src = "//! Doc.\n\nfn a() {}\n    // note\n/// item doc\nfn b() {\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\nfn after() {}\n";
        assert_eq!(code_lines(src), 4);
    }

    #[test]
    fn an_expectation_counts_once() {
        assert_eq!(expectations("#[expect(clippy::x, reason = \"y\")]\nfn a() {}\n    #![expect(dead_code)]\n"), 2);
    }
}
