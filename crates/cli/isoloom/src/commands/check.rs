//! `isoloom check`: whether the generated files under .isoloom/ still match the spec (for CI).

use std::path::PathBuf;

use super::{Console, Project, Result, Verdict};
use crate::{generated, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    /// Only this target's files (any that `isoloom targets` lists); a profile's pinned target
    /// when it has one.
    #[arg(long)]
    target: Option<String>,
    /// The image table the files were generated with (see `generate --images`).
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// Generates the targets `generate` would (every one, or the one asked for or pinned by the
/// profile) in memory and compares each file with the one on disk. A target left out because it
/// refuses the lab has no files to compare, unless it is the one asked for: then there is nothing
/// to vouch for. One left out for any other reason (its backend failed) is an error, since its
/// committed files can't be vouched for.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let target = spec::pinned(profile, &a.project.dir, a.target.as_deref())?;
    let (lab, _) = spec::with_settings(profile, &a.project.dir, a.images.as_deref(), &a.sets)?;
    let lab = spec::checked(lab, &a.project.dir)?;
    let (files, skipped) = super::generate::files(&lab, target.as_deref())?;
    if target.is_some() && files.is_empty() {
        for e in &skipped {
            console.line(format_args!("✗ {e}"))?;
        }
        return Ok(Verdict::of(false));
    }
    let files = generated::finish(files, &a.project.dir)?;
    let mut stale = 0;
    for f in &files {
        match std::fs::read_to_string(a.project.dir.join(&f.path)) {
            Ok(current) if current == f.contents => console.line(format_args!("✓ {}", f.path))?,
            Ok(_) => {
                console.line(format_args!("✗ {} is out of date", f.path))?;
                stale += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                console.line(format_args!("✗ {} is missing", f.path))?;
                stale += 1;
            }
            Err(e) => {
                console.line(format_args!("✗ {} can't be read: {e}", f.path))?;
                stale += 1;
            }
        }
    }
    if stale > 0 {
        console.line("run `isoloom generate` and commit the result")?;
    }
    Ok(Verdict::of(stale == 0))
}
