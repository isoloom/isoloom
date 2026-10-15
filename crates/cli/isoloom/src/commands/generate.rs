//! `isoloom generate`: each target's files under .isoloom/, or one target's.

use std::path::PathBuf;

use super::{Console, Project, Result, Verdict};
use crate::{generated, spec, target};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    /// Only this target (docker, vagrant).
    #[arg(long)]
    target: Option<String>,
    /// Your own image table (YAML): OS images, and the access machine when the spec leaves
    /// it to the runner. See <https://www.isoloom.com/en/docs/images>
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// Writes the files, then says why each target left out wasn't generated. Asking for one target
/// that can't be generated is a no; skipping some of all of them isn't.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let dir = &a.project.dir;
    let target = spec::pinned(profile, dir, a.target.as_deref())?;
    let (lab, settings) = spec::with_settings(profile, dir, a.images.as_deref(), &a.sets)?;
    let lab = spec::checked(lab, dir)?;
    let (files, skipped) = match &target {
        Some(id) => match isoloom_targets::generate(&lab, target::by_id(id)?) {
            Ok(f) => (f, vec![]),
            Err(isoloom_targets::Error::Refused(e)) => (vec![], vec![e]),
            Err(e) => return Err(e.into()),
        },
        None => isoloom_targets::generate_all(&lab),
    };
    let files = generated::finish(files, &settings.defaults, dir)?;
    for f in &files {
        generated::write_file(dir, f)?;
        console.line(format_args!("✓ {}", f.path))?;
    }
    for e in &skipped {
        console.line(format_args!("· {e}"))?;
    }
    Ok(Verdict::of(target.is_none() || !files.is_empty()))
}
