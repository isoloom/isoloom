//! `isoloom generate`: each target's files under .isoloom/, or one target's.

use std::path::Path;

use isoloom_targets::{GenerateError, GeneratedFile, Lab};

use super::{Console, Project, Result, Verdict};
use crate::{generated, spec, target};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    #[command(flatten)]
    generation: Generation,
}

// What `generate` and `check` work the files out with, flattened into their arguments (no doc
// comment: clap would print it in `--help`).
#[derive(Debug, clap::Args)]
pub(crate) struct Generation {
    /// Only this target (any that `isoloom targets` lists); a profile's pinned target when it
    /// has one.
    #[arg(long)]
    target: Option<String>,
    #[command(flatten)]
    settings: spec::Settings,
}

/// What `generate` writes and removes, worked out before anything is.
pub(super) struct Planned {
    /// The files, with the project copies the targets add; none when the target asked for, or
    /// every target, refuses the lab.
    pub(super) files: Vec<GeneratedFile>,
    /// The paths Isoloom wrote before and no longer writes, in the generation's scope.
    pub(super) stale: Vec<String>,
    /// The list of what Isoloom wrote (`.isoloom/.generated`), when it changes.
    pub(super) list: Option<GeneratedFile>,
    /// The refusals of the targets left out.
    pub(super) skipped: Vec<GenerateError>,
}

/// The files `generation` asks for in project `dir`, read as `generate` reads it (the profile's
/// pinned target, the image table and `-s` overrides), and what writing them changes: the files
/// no longer generated in its scope (every target's `.isoloom/`, or the target's folder), and the
/// list of what Isoloom wrote. Refused when that list names a path Isoloom won't remove, or a
/// file would be written through a symbolic link. Nothing is read from the outputs when no file
/// is generated.
pub(super) fn planned(generation: &Generation, profile: &spec::Profile, dir: &Path) -> Result<Planned> {
    let target = spec::pinned(profile, dir, generation.target.as_deref())?;
    let (lab, _) = spec::with_settings(profile, dir, &generation.settings)?;
    let lab = spec::checked(lab, dir)?;
    let (files, skipped) = files(&lab, target.as_deref())?;
    if files.is_empty() {
        return Ok(Planned {
            files,
            stale: Vec::new(),
            list: None,
            skipped,
        });
    }
    let files = generated::finish(files, dir)?;
    let stale = generated::stale(dir, &files, &generated::scope(target.as_deref()))?;
    let list = generated::list(dir, &files, &stale)?;
    generated::writable(dir, files.iter().chain(&list))?;
    Ok(Planned {
        files,
        stale,
        list,
        skipped,
    })
}

/// Writes the files, removes those Isoloom wrote before and no longer writes (in the target's
/// folder, or anywhere under `.isoloom/` for every target), then says why each target left out
/// wasn't generated; nothing is written before all of it is worked out ([`planned`]). A
/// generation with no file (the target asked for, or every target, refuses the lab) is a no, and
/// writes and removes nothing: the files there stay as they were. Skipping some of all the
/// targets isn't a no, unless one was left out for a failure of its backend rather than a refusal
/// of the lab: then nothing is written or removed.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let dir = &a.project.dir;
    let Planned {
        files,
        stale,
        list,
        skipped,
    } = planned(&a.generation, profile, dir)?;
    if files.is_empty() {
        for e in &skipped {
            console.line(format_args!("· {e}"))?;
        }
        return Ok(Verdict::Fail);
    }
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
fn files(lab: &Lab, target: Option<&str>) -> Result<(Vec<GeneratedFile>, Vec<GenerateError>)> {
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
