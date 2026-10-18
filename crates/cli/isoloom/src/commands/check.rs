//! `isoloom check`: whether the generated files under .isoloom/ still match the spec (for CI).

use std::path::PathBuf;

use super::{Console, Project, Result, Verdict};
use crate::{generated, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    /// The image table the files were generated with (see `generate --images`).
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// Generates every target in memory and compares each file with the one on disk.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let (lab, settings) = spec::with_settings(profile, &a.project.dir, a.images.as_deref(), &a.sets)?;
    let lab = spec::checked(lab, &a.project.dir)?;
    let (files, _) = isoloom_targets::generate_all(&lab);
    let files = generated::finish(files, &settings.defaults, &a.project.dir)?;
    let mut stale = 0;
    for f in &files {
        match std::fs::read_to_string(a.project.dir.join(&f.path)) {
            Ok(current) if current == f.contents => console.line(format_args!("✓ {}", f.path))?,
            Ok(_) => {
                console.line(format_args!("✗ {} is out of date", f.path))?;
                stale += 1;
            }
            Err(_) => {
                console.line(format_args!("✗ {} is missing", f.path))?;
                stale += 1;
            }
        }
    }
    if stale > 0 {
        console.line("run `isoloom generate` and commit the result")?;
    }
    Ok(Verdict::of(stale == 0))
}
