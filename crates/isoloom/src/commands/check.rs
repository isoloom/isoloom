//! `isoloom check`: whether the generated files under .isoloom/ still match the spec (for CI).

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, generated, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The image table the files were generated with (see `generate --images`).
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
    /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let (spec, settings) = spec::load_settings(&a.dir, a.images.as_deref(), &a.sets)?;
    if spec::report_invalid(&spec) {
        return Ok(ExitCode::FAILURE);
    }
    let (files, _) = core::generate_all(&spec);
    let files = generated::finish(files, &settings.defaults, &a.dir)?;
    let mut stale = 0;
    for f in &files {
        match std::fs::read_to_string(a.dir.join(&f.path)) {
            Ok(current) if current == f.contents => println!("✓ {}", f.path),
            Ok(_) => {
                println!("✗ {} is out of date", f.path);
                stale += 1;
            }
            Err(_) => {
                println!("✗ {} is missing", f.path);
                stale += 1;
            }
        }
    }
    if stale > 0 {
        println!("run `isoloom generate` and commit the result");
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}
