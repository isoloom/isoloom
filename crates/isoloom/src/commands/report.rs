//! `isoloom report <name>`: a table about the environment, as text or Markdown.

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, spec};

#[derive(clap::Args)]
pub struct Args {
    /// The report.
    name: String,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Markdown instead of aligned text.
    #[arg(long)]
    md: bool,
    /// Report on instance N.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Overrides, as for `generate`.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    if a.name == "list" {
        for (n, what) in core::report::REPORTS {
            println!("{n:<12} {what}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let snapshot = spec::snapshot_of(&a.dir, a.instance, &a.sets)?;
    print!("{}", core::report::report(&a.name, &snapshot, a.md)?);
    Ok(ExitCode::SUCCESS)
}
