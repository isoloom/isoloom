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
    let names = unused(&String::from_utf8_lossy(&out.stdout));
    if !measured(out.status.success(), out.status.code(), &names) {
        return Err(format!(
            "cargo machete failed ({}), so nothing was counted: is it installed? (cargo install cargo-machete)",
            out.status
        )
        .into());
    }
    for name in names {
        add(counts, "machete/unused", &name);
    }
    for c in crates {
        let path = path_in(root, &c.dir).join("Cargo.toml");
        let manifest = std::fs::read_to_string(&path)?;
        let ignored = ignored(&manifest).map_err(|e| format!("{}: {e}", path.display()))?;
        for _ in 0..ignored {
            add(counts, "machete/ignored", &c.name);
        }
    }
    Ok(())
}

/// Whether machete's run measured anything: it exits 0 with nothing unused and 1 with a report of
/// what is; any other end (cargo's own failure for a missing subcommand, machete's error) counted
/// as zero would let the ratchet pass on nothing.
fn measured(success: bool, code: Option<i32>, unused: &[String]) -> bool {
    success || (code == Some(1) && !unused.is_empty())
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
///
/// # Errors
///
/// A manifest that doesn't parse: counted as none, the ratchet would go down instead of failing.
fn ignored(manifest: &str) -> Result<usize, toml::de::Error> {
    let doc = manifest.parse::<toml::Table>()?;
    Ok(doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("cargo-machete"))
        .and_then(|m| m.get("ignored"))
        .and_then(toml::Value::as_array)
        .map_or(0, Vec::len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_unused_dependency_is_named_with_its_crate() {
        let report = "cargo-machete found the following unused dependencies in this directory:\npkg-x -- ./crates/core/pkg-x/Cargo.toml:\n\tserde\n\tindexmap\n\nDone!\n";
        assert_eq!(unused(report), ["pkg-x", "pkg-x"]);
        assert_eq!(
            ignored("[package]\nname = \"a\"\n[package.metadata.cargo-machete]\nignored = [\"indexmap\"]\n").unwrap(),
            1
        );
    }

    #[test]
    fn a_machete_that_didnt_run_or_a_manifest_that_doesnt_parse_fails_the_count() {
        let found = ["pkg-x".to_owned()];
        assert!(measured(true, Some(0), &[]));
        assert!(measured(false, Some(1), &found));
        // Regression: cargo without machete installed (exit 101, nothing on stdout) counted 0.
        assert!(!measured(false, Some(101), &[]));
        assert!(!measured(false, Some(1), &[]));
        assert!(ignored("[package\n").is_err());
    }
}
