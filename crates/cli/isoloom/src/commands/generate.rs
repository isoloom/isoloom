//! `isoloom generate`: each target's files under .isoloom/, or one target's.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Res, generated, output, spec, target};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
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

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let dir = &a.dir;
    let target = spec::pinned(profile, dir, a.target.as_deref())?;
    let (lab, settings) = spec::load_settings(profile, dir, a.images.as_deref(), &a.sets)?;
    if spec::report_invalid(dir, &lab) {
        return Ok(ExitCode::FAILURE);
    }
    let (files, skipped) = match &target {
        Some(id) => match isoloom_targets::generate(&lab, target::by_id(id)?) {
            Ok(f) => (f, vec![]),
            Err(e) => (vec![], vec![e]),
        },
        None => isoloom_targets::generate_all(&lab),
    };
    let files = generated::finish(files, &settings.defaults, dir)?;
    for f in &files {
        generated::write_file(dir, f)?;
        println!("✓ {}", f.path);
    }
    for e in &skipped {
        println!("· {e}");
    }
    // Asking for one target that can't be generated is an error; skipping some of "all" isn't.
    Ok(output::exit_code(target.is_none() || !files.is_empty()))
}
