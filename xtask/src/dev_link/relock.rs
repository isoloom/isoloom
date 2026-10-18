//! `cargo xtask dev-link relock`: a consumer's committed Cargo.lock brought up to date with the
//! Isoloom commits its manifests pin, fetched from this checkout instead of GitHub (the commits
//! of the local main aren't pushed, so GitHub doesn't have them).
//!
//! For each workspace of the consumer whose Cargo.lock pins Isoloom by git, it reads the commits
//! its manifests pin (`rev = "..."`), checks this checkout has each, then runs `cargo update
//! --workspace` there with git's `url.<this checkout>.insteadOf` set for the Isoloom URL and git's
//! own fetch (`CARGO_NET_GIT_FETCH_WITH_CLI`): cargo's cache of the GitHub repository gets the
//! commits from here, the lockfile keeps the GitHub URL, and only what the new commits need
//! changes in it. Cargo runs from a folder outside the consumer, so a dev-link config there
//! (the `[patch]` and its own lockfile) stays out of it.
//!
//! The usual move of a pin: set the new commit in the consumer's manifests, then relock.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use super::GIT_URL;
use crate::{Res, process, repo};

/// Relocks every workspace of consumer `c` that pins Isoloom by git; a line per workspace.
pub(super) fn relock(root: &Path, c: &Path, workspaces: &[String]) -> Res<Vec<String>> {
    if workspaces.is_empty() {
        return Ok(vec!["no workspace pins Isoloom by git: nothing to relock".into()]);
    }
    let mut lines = Vec::new();
    for ws in workspaces {
        let manifest = c.join(ws).join("Cargo.toml");
        let revs = pinned_revs(&manifest)?;
        if let Some(missing) = revs.iter().find(|rev| !has_commit(root, rev)) {
            return Err(format!("{ws}: pins {missing}, which {} doesn't have", root.display()).into());
        }
        let lock = c.join(ws).join("Cargo.lock");
        let before = std::fs::read(&lock).unwrap_or_default();
        update(root, &manifest)?;
        let changed = std::fs::read(&lock).unwrap_or_default() != before;
        let pins: Vec<&str> = revs.iter().map(|r| r.get(..10).unwrap_or(r)).collect();
        let what = if changed { "Cargo.lock updated" } else { "Cargo.lock already up to date" };
        lines.push(format!("{ws}: {what} (Isoloom at {})", pins.join(", ")));
    }
    Ok(lines)
}

/// The commits of Isoloom the workspace's manifests pin, from `cargo metadata --no-deps`.
fn pinned_revs(manifest: &Path) -> Res<BTreeSet<String>> {
    let mut cmd = process::cargo(&std::env::temp_dir());
    cmd.args(["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path"])
        .arg(manifest);
    let metadata: serde_json::Value = serde_json::from_str(&process::output(&mut cmd)?)?;
    let sources = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| p.get("dependencies").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|d| d.get("source").and_then(serde_json::Value::as_str));
    Ok(sources.filter_map(isoloom_rev).collect())
}

/// The commit a dependency's source pins, when it is Isoloom's repository by git with a `rev`.
fn isoloom_rev(source: &str) -> Option<String> {
    let query = source.strip_prefix("git+")?.strip_prefix(GIT_URL)?.strip_prefix('?')?;
    query.split('&').find_map(|pair| pair.strip_prefix("rev=")).map(str::to_owned)
}

/// Whether this checkout has commit `rev`.
fn has_commit(root: &Path, rev: &str) -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args(["cat-file", "-e", &format!("{rev}^{{commit}}")]);
    process::quiet(&mut cmd).is_ok()
}

/// `cargo update --workspace` on the workspace of `manifest`, with Isoloom's URL fetched from
/// `root`.
fn update(root: &Path, manifest: &Path) -> Res<()> {
    let mut cmd = process::cargo(&std::env::temp_dir());
    cmd.args(["update", "--workspace", "--manifest-path"]).arg(manifest);
    cmd.env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", format!("url.{}.insteadOf", repo::slashed(root)))
        .env("GIT_CONFIG_VALUE_0", GIT_URL);
    process::run(&mut cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_rev_of_isoloom_itself_is_a_pin() {
        let rev = "2201afe561d5dd9de91694caf1b245ed2bb79b57";
        assert_eq!(isoloom_rev(&format!("git+{GIT_URL}?rev={rev}")).as_deref(), Some(rev));
        assert_eq!(isoloom_rev(&format!("git+{GIT_URL}?branch=main&rev={rev}")).as_deref(), Some(rev));
        assert_eq!(isoloom_rev(&format!("git+{GIT_URL}-website?rev={rev}")), None);
        assert_eq!(isoloom_rev(&format!("git+{GIT_URL}?branch=main")), None);
        assert_eq!(isoloom_rev("registry+https://github.com/rust-lang/crates.io-index"), None);
    }
}
