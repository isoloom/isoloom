//! `isoloom check`: whether the generated files under .isoloom/ still match the spec (for CI).

use std::path::Path;

use isoloom_targets::GeneratedFile;

use super::generate::{Generation, Planned, planned};
use super::{Console, Project, Result, Verdict};
use crate::spec;

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    #[command(flatten)]
    generation: Generation,
    /// Only the files that fail, so a CI log shows what to fix.
    #[arg(long, short = 'q', conflicts_with = "json")]
    quiet: bool,
    /// Machine-readable output: one result per file, then a summary.
    #[arg(long)]
    json: bool,
}

/// What `check` found of one file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    /// As `generate` would write it.
    Current,
    /// Different.
    OutOfDate,
    /// Not there.
    Missing,
    /// There, but not readable as a file.
    Unreadable,
    /// Written by an earlier `generate`, which would now remove it.
    NoLongerGenerated,
}

/// One file's result.
#[derive(Debug, Clone, serde::Serialize)]
struct FileResult {
    /// Relative to the project folder.
    path: String,
    status: Status,
    /// Why it can't be read (only for [`Status::Unreadable`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl FileResult {
    fn passed(&self) -> bool {
        self.status == Status::Current
    }

    /// Its line in the text report.
    fn line(&self) -> String {
        let path = &self.path;
        match (&self.status, &self.reason) {
            (Status::Current, _) => format!("✓ {path}"),
            (Status::OutOfDate, _) => format!("✗ {path} is out of date"),
            (Status::Missing, _) => format!("✗ {path} is missing"),
            (Status::Unreadable, reason) => format!("✗ {path} can't be read: {}", reason.as_deref().unwrap_or_default()),
            (Status::NoLongerGenerated, _) => format!("✗ {path} is no longer generated"),
        }
    }
}

/// Generates the targets `generate` would (every one, or the one asked for or pinned by the
/// profile) in memory and compares each file with the one on disk, the list of what it wrote
/// (`.isoloom/.generated`) included; a file `generate` wrote there before and would now remove
/// (`generated::stale`) is out of date too; one `generate` would refuse to write through a
/// symbolic link is the same error as there. A target left out because it refuses the environment has no
/// files to compare; when no target is left (the one asked for, or every one, refuses), there is
/// nothing to vouch for: a no. One left out for any other reason (its backend failed) is an
/// error, since its committed files can't be vouched for.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let Planned { files, stale, list, skipped } = planned(&a.generation, profile, &a.project.dir)?;
    if files.is_empty() {
        let refused: Vec<String> = skipped.iter().map(ToString::to_string).collect();
        report(a, console, &[], &refused)?;
        return Ok(Verdict::Fail);
    }
    let mut results: Vec<FileResult> = files.iter().chain(&list).map(|f| compare(&a.project.dir, f)).collect();
    results.extend(stale.into_iter().map(|path| FileResult {
        path,
        status: Status::NoLongerGenerated,
        reason: None,
    }));
    report(a, console, &results, &[])?;
    Ok(Verdict::of(results.iter().all(FileResult::passed)))
}

/// `f` against the file on disk under `dir`.
fn compare(dir: &Path, f: &GeneratedFile) -> FileResult {
    let (status, reason) = match std::fs::read_to_string(dir.join(&f.path)) {
        Ok(current) if current == f.contents => (Status::Current, None),
        Ok(_) => (Status::OutOfDate, None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Status::Missing, None),
        Err(e) => (Status::Unreadable, Some(e.to_string())),
    };
    FileResult {
        path: f.path.clone(),
        status,
        reason,
    }
}

/// The results as `--json`, `--quiet` or the full text report asks: `refused` are the reasons
/// the one target asked for has no files.
fn report(a: &Args, console: &mut Console, results: &[FileResult], refused: &[String]) -> Result<()> {
    let failed = results.iter().filter(|r| !r.passed()).count();
    let ok = failed == 0 && refused.is_empty();
    if a.json {
        return console.json(&serde_json::json!({
            "files": results,
            "refused": refused,
            "summary": { "ok": ok, "checked": results.len(), "current": results.len() - failed, "failed": failed },
        }));
    }
    for reason in refused {
        console.line(format_args!("✗ {reason}"))?;
    }
    for r in results.iter().filter(|r| !a.quiet || !r.passed()) {
        console.line(r.line())?;
    }
    if failed > 0 {
        console.line("run `isoloom generate` and commit the result")?;
    } else if a.quiet && ok {
        console.line(format_args!("✓ {} files up to date", results.len()))?;
    }
    Ok(())
}
