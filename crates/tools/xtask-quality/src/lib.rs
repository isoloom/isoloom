//! The shared `cargo xtask quality` ratchet, used by Isoloom and the repositories that build on it.
//!
//! It is the ratchet of the engineering standard: every count of a clean-code rule, per crate,
//! against a baseline file ([`QualityConfig::baseline`]). A count that goes up fails; one that goes
//! down is written down with `--update`, which never raises one. A missing baseline fails too:
//! only `--init` writes the first one, so deleting it never lets every count through.
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
//! [`QualityConfig`]: what differs between the repositories (the jscpd temporary folder's
//! prefix, the command named in the "went down" hint, and where the baseline lives).

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
    /// Write the first baseline, when there is none.
    #[arg(long, conflicts_with = "update")]
    init: bool,
}

/// What a repository's `xtask` tells the ratchet about itself: what differs between
/// repositories.
#[derive(Debug, Clone, Copy)]
pub struct QualityConfig {
    /// The prefix of the jscpd temporary folder (`<slug>-jscpd-<pid>`), kept per repository so two
    /// repositories' runs never share one.
    pub jscpd_slug: &'static str,
    /// The command a "went down" message tells the maintainer to run (`--update` is appended),
    /// e.g. `cargo xtask quality` or `cargo run -p xtask -- quality`.
    pub update_hint: &'static str,
    /// The baseline file, relative to the root with `/` (`xtask/baselines/quality-baseline.json`).
    pub baseline: &'static str,
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
/// baseline ([`QualityConfig::baseline`]), as `config` describes the repository. Returns what to show and
/// whether it passed; it prints nothing itself.
///
/// # Errors
///
/// Fails when a tool it drives (clippy, jscpd through npx, cargo-machete, cargo-metadata) can't
/// run, a source folder can't be read, or the baseline file can't be read or written.
pub fn run(a: &Args, root: &Path, config: &QualityConfig) -> Res<Outcome> {
    let counts = measure(root, config)?;
    against_baseline(a, &path_in(root, config.baseline), &counts, config)
}

/// Every count of the `root` workspace.
fn measure(root: &Path, config: &QualityConfig) -> Res<Counts> {
    let crates = sources::crates(root)?;
    let mut counts = Counts::new();
    clippy::measure(root, &crates, &mut counts)?;
    sources::measure(root, &crates, &mut counts)?;
    dup::measure(root, &crates, &mut counts, config.jscpd_slug)?;
    machete::measure(root, &crates, &mut counts)?;
    Ok(counts)
}

/// `counts` against the baseline `file`: written there with `--init` when there is none, else a
/// failure; compared with it, and rewritten with `--update` when no count went up.
fn against_baseline(a: &Args, file: &Path, counts: &Counts, config: &QualityConfig) -> Res<Outcome> {
    let mut out = String::new();
    let Some(before) = baseline::read(file)? else {
        if !a.init {
            writeln!(
                out,
                "quality: no baseline at {}; `{} --init` writes the first one",
                file.display(),
                config.update_hint
            )?;
            return Ok(Outcome { output: out, success: false });
        }
        out.push_str(&baseline::table(counts));
        baseline::write(file, counts)?;
        writeln!(out, "quality: {} written", file.display())?;
        return Ok(Outcome { output: out, success: true });
    };
    if a.init {
        writeln!(out, "quality: {} is there already; `--update` lowers its counts", file.display())?;
        return Ok(Outcome { output: out, success: false });
    }
    let changes = baseline::compare(&before, counts);
    if a.table || a.update {
        out.push_str(&baseline::table(counts));
    }
    for line in &changes.lines {
        writeln!(out, "{line}")?;
    }
    if !changes.raised.is_empty() {
        writeln!(out, "quality: {} count(s) went up; bring them back down (see above)", changes.raised.len())?;
        return Ok(Outcome { output: out, success: false });
    }
    if a.update {
        baseline::write(file, counts)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: QualityConfig = QualityConfig {
        jscpd_slug: "test",
        update_hint: "cargo xtask quality",
        baseline: "baseline.json",
    };

    fn args(init: bool, update: bool) -> Args {
        Args { update, table: false, init }
    }

    /// A fresh folder for one test, removed when it goes out of scope.
    fn folder(name: &str) -> Res<tempfile::TempDir> {
        Ok(tempfile::Builder::new().prefix(&format!("xtask-quality-{name}-")).tempdir()?)
    }

    #[test]
    fn a_missing_baseline_fails_unless_init_writes_it() -> Res<()> {
        let dir = folder("missing")?;
        let dir = dir.path();
        let file = dir.join("baseline.json");
        let mut counts = Counts::new();
        add(&mut counts, "clippy/x", "a");
        let refused = against_baseline(&args(false, false), &file, &counts, &CONFIG)?;
        assert!(
            !refused.success && refused.output.contains("--init` writes the first one"),
            "{}",
            refused.output
        );
        assert!(!file.exists(), "nothing is written without --init");
        let first = against_baseline(&args(true, false), &file, &counts, &CONFIG)?;
        assert!(first.success && file.exists(), "{}", first.output);
        // Once there, --init doesn't rewrite it; the counts are checked against it.
        add(&mut counts, "clippy/x", "a");
        assert!(!against_baseline(&args(true, false), &file, &counts, &CONFIG)?.success);
        assert!(!against_baseline(&args(false, false), &file, &counts, &CONFIG)?.success, "a count went up");
        Ok(())
    }
}
