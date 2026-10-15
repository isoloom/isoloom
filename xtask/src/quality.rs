//! `cargo xtask quality`: the ratchet of the engineering standard (PLAN 3b, 1c). Every count of
//! a clean-code rule, per crate, against `quality-baseline.json`: a count that goes up fails;
//! one that goes down is written down with `--update`, which never raises one.
//!
//! What is counted, per crate:
//! - every warning `cargo clippy --workspace --all-targets` gives (`clippy/<lint>`,
//!   `rustc/<lint>`): the lints the workspace's `[lints]` table holds at warn, because their
//!   count isn't zero yet (the others are at deny, and fail the build);
//! - source files over 400 lines of code (`file/over-400-loc`);
//! - blocks of code found twice (`dup/clone`, jscpd over the Rust sources);
//! - dependencies nothing uses (`machete/unused`) and the ones machete is told to ignore
//!   (`machete/ignored`);
//! - the exceptions, `#[expect(lint, reason = "…")]` (`exception/expect`): each is reviewed, so a
//!   new one raises a count too.

mod baseline;
mod clippy;
mod dup;
mod machete;
mod sources;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, repo};

/// Each rule's count per crate (the crates with none left out).
pub(crate) type Counts = BTreeMap<String, BTreeMap<String, u64>>;

/// The arguments of `cargo xtask quality`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the counts into the baseline (only when none went up).
    #[arg(long)]
    update: bool,
    /// Print every count, rule by rule and crate by crate.
    #[arg(long)]
    table: bool,
}

/// A crate of the workspace: its name and folder, relative to the root with `/`.
#[derive(Debug, Clone)]
pub(crate) struct Crate {
    /// Its package name.
    pub(crate) name: String,
    /// Its folder, relative to the workspace root.
    pub(crate) dir: String,
}

/// The crate holding `file` (relative to the root, with `/`): the one with the longest folder
/// that contains it, else `workspace`.
pub(crate) fn crate_of(crates: &[Crate], file: &str) -> String {
    crates
        .iter()
        .filter(|c| file.starts_with(&format!("{}/", c.dir)))
        .max_by_key(|c| c.dir.len())
        .map_or_else(|| "workspace".to_owned(), |c| c.name.clone())
}

/// Adds one finding of `rule` in `krate`.
pub(crate) fn add(counts: &mut Counts, rule: &str, krate: &str) {
    *counts.entry(rule.to_owned()).or_default().entry(krate.to_owned()).or_default() += 1;
}

/// Measures, then checks against (or with `--update`, rewrites) the baseline.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let crates = sources::crates(&root)?;
    let mut counts = Counts::new();
    clippy::measure(&root, &crates, &mut counts)?;
    sources::measure(&root, &crates, &mut counts)?;
    dup::measure(&root, &crates, &mut counts)?;
    machete::measure(&root, &crates, &mut counts)?;
    let file = root.join("quality-baseline.json");
    let Some(before) = baseline::read(&file)? else {
        // The first measure is the baseline.
        print!("{}", baseline::table(&counts));
        baseline::write(&file, &counts)?;
        println!("quality: no baseline yet; {} written", file.display());
        return Ok(ExitCode::SUCCESS);
    };
    let changes = baseline::compare(&before, &counts);
    if a.table || a.update {
        print!("{}", baseline::table(&counts));
    }
    for line in &changes.lines {
        println!("{line}");
    }
    if !changes.raised.is_empty() {
        println!("quality: {} count(s) went up; bring them back down (see above)", changes.raised.len());
        return Ok(ExitCode::FAILURE);
    }
    if a.update {
        baseline::write(&file, &counts)?;
        println!("quality: {} written", file.display());
    } else if !changes.lowered.is_empty() {
        println!(
            "quality: {} count(s) went down: `cargo xtask quality --update` writes them",
            changes.lowered.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// `root/dir` as a path.
pub(crate) fn path_in(root: &Path, dir: &str) -> PathBuf {
    dir.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}
