//! `cargo xtask dev-link`: points the consumers' Isoloom dependencies at this checkout, for local
//! work across the repositories, without editing their committed Cargo.toml pins or Cargo.lock.
//!
//! The consumers default to the Isothrum and launcher checkouts next to the main Isoloom
//! checkout; `ISOLOOM_ISOTHRUM_DIR` and `ISOLOOM_LAUNCHER_DIR` override them.
//!
//! Each workspace of a consumer whose committed Cargo.lock pins Isoloom by git gets a
//! `.cargo/config.toml` with:
//! - a `[patch."https://github.com/isoloom/isoloom"]` entry per Isoloom crate the workspace
//!   depends on directly, pointing at its folder here (`crates/<group>/<name>`; the others
//!   follow through the path dependencies inside this workspace);
//! - a `resolver.lockfile-path` to a lockfile of its own, seeded from the committed one, so the
//!   committed Cargo.lock keeps the git pin.
//!
//! Cargo reads `.cargo/config.toml` in the folder it runs from and its parents. So the workspace
//! at the repository root, or the only one, gets its file at the repository root (the launcher
//! runs `cargo --manifest-path src-tauri/Cargo.toml` from there); any other workspace gets one in
//! its own folder. Their paths go to the repository's `.git/info/exclude`: local, never
//! committed, and shared by every branch and worktree, so they stay there after an unlink
//! (another worktree may still be linked). Running link or unlink again gives the same result.
//!
//! `relock` brings a consumer's committed Cargo.lock up to date with the Isoloom commits its
//! manifests pin, fetched from this checkout instead of GitHub ([`relock`]).

mod config;
mod lockfile;
mod relock;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::{Res, process, repo};

pub(crate) use config::{ALT_DIR, linked_configs, linked_isoloom};

/// The repository the consumers pin Isoloom from.
pub(crate) const GIT_URL: &str = "https://github.com/isoloom/isoloom";

/// What to do with the consumers' links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Action {
    /// Build against this checkout's crates.
    Link,
    /// Back to the git pins.
    Unlink,
    /// Say which consumers are linked, and to what.
    Status,
    /// Update the consumers' committed Cargo.lock to the Isoloom commits their manifests pin,
    /// fetched from this checkout instead of GitHub.
    Relock,
}

/// The arguments of `cargo xtask dev-link`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// link, unlink, status or relock.
    #[arg(value_enum)]
    action: Action,
    /// The consumers' checkouts (default: Isothrum and the launcher next to the main checkout).
    consumers: Vec<PathBuf>,
}

/// Links, unlinks, reports or relocks each consumer; fails when one couldn't be.
pub(crate) fn run(a: &Args) -> ExitCode {
    let root = repo::root();
    let consumers = if a.consumers.is_empty() {
        let code = repo::code_dir(&root);
        vec![
            repo::consumer("ISOLOOM_ISOTHRUM_DIR", &code, "isothrum"),
            repo::consumer("ISOLOOM_LAUNCHER_DIR", &code, "cyber-ctf-launcher"),
        ]
    } else {
        a.consumers.clone()
    };
    let mut ok = true;
    for c in consumers {
        if !c.join(".git").exists() {
            println!("{}: not a git checkout, skipped", c.display());
            continue;
        }
        let c = repo::physical(&c);
        println!("{}:", c.display());
        match a.action {
            Action::Link => ok &= report(link(&root, &c)),
            Action::Unlink => ok &= report(config::unlink(&c).map(|()| vec!["git pins (not linked)".into()])),
            Action::Status => ok &= report(Ok(config::status(&c))),
            Action::Relock => ok &= report(workspaces(&c).and_then(|list| relock::relock(&root, &c, &list))),
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Prints each line of what happened, or the error; whether it went well.
fn report(done: Res<Vec<String>>) -> bool {
    match done {
        Ok(lines) => {
            for l in &lines {
                println!("  {l}");
            }
            true
        }
        Err(e) => {
            println!("  {e}");
            false
        }
    }
}

/// The workspaces of consumer `c` (folders relative to it, `.` for its root) whose committed
/// Cargo.lock pins Isoloom by git.
fn workspaces(c: &Path) -> Res<Vec<String>> {
    let listed = process::output(Command::new("git").arg("-C").arg(c).args(["ls-files", "--", "Cargo.lock", "*/Cargo.lock"]))?;
    Ok(listed
        .lines()
        .filter(|lock| std::fs::read_to_string(c.join(lock)).is_ok_and(|text| lockfile::pins_git(&text, GIT_URL)))
        .map(|lock| {
            Path::new(lock)
                .parent()
                .map_or(".".into(), |p| if p.as_os_str().is_empty() { ".".into() } else { repo::slashed(p) })
        })
        .collect())
}

/// This checkout's folder of crate `name` (`crates/<group>/<name>`, or `crates/<name>`).
fn local_crate(root: &Path, name: &str) -> Option<PathBuf> {
    let crates = root.join("crates");
    let groups = std::fs::read_dir(&crates).ok()?.flatten().map(|e| e.path());
    std::iter::once(crates.clone())
        .chain(groups)
        .map(|g| g.join(name))
        .find(|dir| lockfile::package_name(&dir.join("Cargo.toml")).as_deref() == Some(name))
}

/// Links consumer `c`: a config per workspace that pins Isoloom by git.
fn link(root: &Path, c: &Path) -> Res<Vec<String>> {
    let list = workspaces(c)?;
    if list.is_empty() {
        config::unlink(c)?;
        return Ok(vec!["no workspace pins Isoloom by git: nothing to link".into()]);
    }
    let at = |ws: &str| if ws == "." || list.len() == 1 { ".".to_owned() } else { ws.to_owned() };
    for ws in &list {
        config::guard(c, &config::config_path(&at(ws)))?;
    }
    config::unlink(c)?;
    let mut lines = Vec::new();
    let mut excludes = Vec::new();
    for ws in &list {
        let text = std::fs::read_to_string(c.join(ws).join("Cargo.lock"))?;
        let (local, missing): (Vec<String>, Vec<String>) = lockfile::direct_git_crates(&text, GIT_URL)
            .into_iter()
            .partition(|n| local_crate(root, n).is_some());
        if !missing.is_empty() {
            lines.push(format!(
                "{ws}: stays on the git pin for {} (not a crate of {})",
                missing.join(" "),
                root.display()
            ));
        }
        if local.is_empty() {
            continue;
        }
        let patches: Vec<(String, PathBuf)> = local.iter().filter_map(|n| local_crate(root, n).map(|d| (n.clone(), d))).collect();
        let written = config::write(
            c,
            &config::Linked {
                root,
                at: &at(ws),
                workspace: ws,
                patches: &patches,
            },
        )?;
        excludes.extend(written);
        lines.push(format!("{ws}: {} -> {} ({})", local.join(" "), root.display(), config::config_path(&at(ws))));
    }
    config::add_excludes(c, &excludes)?;
    Ok(lines)
}
