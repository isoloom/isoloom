//! Unused dependencies: what `cargo machete` reports, and what a crate tells it to ignore
//! (`[package.metadata.cargo-machete] ignored`), both counted per crate.

use std::path::Path;
use std::process::Command;

use super::{Counts, Crate, add, path_in};
use crate::Res;

/// Counts the unused and the ignored dependencies per crate.
pub(crate) fn measure(root: &Path, crates: &[Crate], counts: &mut Counts) -> Res<()> {
    let out = crate::process::without_run_env(&mut Command::new("cargo"))
        .arg("machete")
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo machete couldn't start (cargo install cargo-machete): {e}"))?;
    for name in unused(&String::from_utf8_lossy(&out.stdout)) {
        add(counts, "machete/unused", &name);
    }
    for c in crates {
        let manifest = std::fs::read_to_string(path_in(root, &c.dir).join("Cargo.toml"))?;
        for _ in 0..ignored(&manifest) {
            add(counts, "machete/ignored", &c.name);
        }
    }
    Ok(())
}

/// The crate of each unused dependency in machete's report (`<crate> -- <manifest>:` then one
/// indented line per dependency).
fn unused(report: &str) -> Vec<String> {
    let mut current = None;
    let mut out = Vec::new();
    for line in report.lines() {
        if let Some((name, _)) = line.split_once(" -- ") {
            current = Some(name.trim().to_owned());
        } else if line.starts_with(['\t', ' ']) && !line.trim().is_empty() {
            out.extend(current.clone());
        } else {
            current = None;
        }
    }
    out
}

/// How many dependencies a manifest tells machete to ignore.
fn ignored(manifest: &str) -> usize {
    let Ok(doc) = manifest.parse::<toml::Table>() else { return 0 };
    doc.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("cargo-machete"))
        .and_then(|m| m.get("ignored"))
        .and_then(toml::Value::as_array)
        .map_or(0, Vec::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_unused_dependency_is_named_with_its_crate() {
        let report = "cargo-machete found the following unused dependencies in this directory:\nisoloom-x -- ./crates/core/isoloom-x/Cargo.toml:\n\tserde\n\tindexmap\n\nDone!\n";
        assert_eq!(unused(report), ["isoloom-x", "isoloom-x"]);
        assert_eq!(
            ignored("[package]\nname = \"a\"\n[package.metadata.cargo-machete]\nignored = [\"indexmap\"]\n"),
            1
        );
    }
}
