//! The shared `cargo xtask quality` ratchet, used by Isoloom and Isothrum.
//!
//! It is the ratchet of the engineering standard (PLAN 3b, 1c): every count of a clean-code rule,
//! per crate, against `quality-baseline.json`. A count that goes up fails; one that goes down is
//! written down with `--update`, which never raises one.
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
//!
//! Each repository's `xtask` is a thin wrapper that calls [`run`] with its own root and a
//! [`QualityConfig`]: the two things that differ between the repositories (the jscpd temporary
//! folder's prefix, and the command named in the "went down" hint).

mod baseline;
mod clippy;
mod dup;
mod machete;
pub mod process;
pub mod repo;
mod sources;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// This crate's result type: any error as a boxed trait object.
pub type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// What a ratchet run produced: the text to show, and whether it passed. The library builds the
/// text (no printing, since it is not a binary); the caller's `xtask` prints it.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Everything the run would print, already formatted.
    pub output: String,
    /// Whether the run passed (no count went up).
    pub success: bool,
}

/// Each rule's count per crate (the crates with none left out).
pub(crate) type Counts = BTreeMap<String, BTreeMap<String, u64>>;

/// The arguments of `quality`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Write the counts into the baseline (only when none went up).
    #[arg(long)]
    update: bool,
    /// Print every count, rule by rule and crate by crate.
    #[arg(long)]
    table: bool,
}

/// What a repository's `xtask` tells the ratchet about itself: the two strings that differ
/// between repositories.
#[derive(Debug, Clone, Copy)]
pub struct QualityConfig {
    /// The prefix of the jscpd temporary folder (`<slug>-jscpd-<pid>`), kept per repository so two
    /// repositories' runs never share one.
    pub jscpd_slug: &'static str,
    /// The command a "went down" message tells the maintainer to run (`--update` is appended),
    /// e.g. `cargo xtask quality` or `cargo run -p xtask -- quality`.
    pub update_hint: &'static str,
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

/// `root/dir` as a path.
pub(crate) fn path_in(root: &Path, dir: &str) -> PathBuf {
    dir.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}

/// Measures the `root` workspace, then checks against (or with `--update`, rewrites) its
/// `quality-baseline.json`, as `config` describes the repository. Returns what to show and
/// whether it passed; it prints nothing itself.
///
/// # Errors
///
/// Fails when a tool it drives (clippy, jscpd through npx, cargo-machete, cargo-metadata) can't
/// run, or the baseline file can't be read or written.
pub fn run(a: &Args, root: &Path, config: &QualityConfig) -> Res<Outcome> {
    let crates = sources::crates(root)?;
    let mut counts = Counts::new();
    clippy::measure(root, &crates, &mut counts)?;
    sources::measure(root, &crates, &mut counts)?;
    dup::measure(root, &crates, &mut counts, config.jscpd_slug)?;
    machete::measure(root, &crates, &mut counts)?;
    let file = root.join("quality-baseline.json");
    let mut out = String::new();
    let Some(before) = baseline::read(&file)? else {
        // The first measure is the baseline.
        out.push_str(&baseline::table(&counts));
        baseline::write(&file, &counts)?;
        writeln!(out, "quality: no baseline yet; {} written", file.display())?;
        return Ok(Outcome { output: out, success: true });
    };
    let changes = baseline::compare(&before, &counts);
    if a.table || a.update {
        out.push_str(&baseline::table(&counts));
    }
    for line in &changes.lines {
        writeln!(out, "{line}")?;
    }
    if !changes.raised.is_empty() {
        writeln!(out, "quality: {} count(s) went up; bring them back down (see above)", changes.raised.len())?;
        return Ok(Outcome { output: out, success: false });
    }
    if a.update {
        baseline::write(&file, &counts)?;
        writeln!(out, "quality: {} written", file.display())?;
    } else if !changes.lowered.is_empty() {
        writeln!(
            out,
            "quality: {} count(s) went down: `{} --update` writes them",
            changes.lowered.len(),
            config.update_hint
        )?;
    }
    Ok(Outcome { output: out, success: true })
}
