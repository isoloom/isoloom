//! `isoloom generate`: each target's files under .isoloom/, or one target's.

use std::path::PathBuf;

use isoloom_targets::{GenerateError, GeneratedFile, Lab};

use super::{Console, Project, Result, Verdict};
use crate::{generated, spec, target};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    /// Only this target (any that `isoloom targets` lists).
    #[arg(long)]
    target: Option<String>,
    /// Your own image table (YAML): OS images, and the access machine when the spec leaves
    /// it to the runner. See <https://www.isoloom.com/en/docs/configuration#the-image-table>
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// Writes the files, removes those Isoloom wrote before and no longer writes (in the target's
/// folder, or anywhere under `.isoloom/` for every target: `generated`), then says why each
/// target left out wasn't generated. The list of what Isoloom wrote is read, and refused when it
/// names a path Isoloom won't remove, and a file that would be written through a symbolic link
/// is refused, before anything is written. A generation with no file (the target asked for, or
/// every target, refuses the lab) is a no, and writes and removes nothing: the files there stay
/// as they were. Skipping some of all the targets isn't a no, unless one was left out for a
/// failure of its backend rather than a refusal of the lab: then nothing is written or removed.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let dir = &a.project.dir;
    let target = spec::pinned(profile, dir, a.target.as_deref())?;
    let (lab, _) = spec::with_settings(profile, dir, a.images.as_deref(), &a.sets)?;
    let lab = spec::checked(lab, dir)?;
    let (files, skipped) = files(&lab, target.as_deref())?;
    if files.is_empty() {
        for e in &skipped {
            console.line(format_args!("· {e}"))?;
        }
        return Ok(Verdict::Fail);
    }
    let files = generated::finish(files, dir)?;
    let stale = generated::stale(dir, &files, &generated::scope(target.as_deref()))?;
    let list = generated::list(dir, &files, &stale)?;
    generated::writable(dir, files.iter().chain(&list))?;
    for f in &files {
        generated::write_file(dir, f)?;
        console.line(format_args!("✓ {}", f.path))?;
    }
    generated::remove(dir, &stale)?;
    for p in &stale {
        console.line(format_args!("− {p} (no longer generated)"))?;
    }
    if let Some(list) = &list {
        generated::write_file(dir, list)?;
    }
    for e in &skipped {
        console.line(format_args!("· {e}"))?;
    }
    Ok(Verdict::Pass)
}

/// `target`'s files, or every target's without one, and the refusals of the targets left out. A
/// target left out for a failure of its backend is the error.
pub(super) fn files(lab: &Lab, target: Option<&str>) -> Result<(Vec<GeneratedFile>, Vec<GenerateError>)> {
    let Some(id) = target else {
        let (files, skipped) = isoloom_targets::generate_all(lab);
        return Ok((files, super::refusals(skipped)?));
    };
    match isoloom_targets::generate(lab, target::by_id(id)?) {
        Ok(f) => Ok((f, vec![])),
        Err(isoloom_targets::Error::Generate(e)) if e.is_refusal() => Ok((vec![], vec![e])),
        Err(e) => Err(e.into()),
    }
}
