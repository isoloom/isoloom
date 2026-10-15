//! `isoloom report <name>`: a table about the environment, as text or Markdown.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

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

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    if a.name == "list" {
        for (n, what) in isoloom_targets::report::REPORTS {
            println!("{n:<12} {what}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let snapshot = spec::snapshot_of(profile, &a.dir, a.instance, &a.sets)?;
    print!("{}", isoloom_targets::report::report(&a.name, &snapshot, a.md)?);
    // Published ports on Hyper-V (the provider the defaults or Vagrant pick): not forwarded.
    if matches!(a.name.as_str(), "ports" | "services")
        && vagrant_provider(profile, &a.dir).as_deref() == Some("hyperv")
        && let Some(note) = isoloom_targets::report::hyperv_ports_note(&snapshot)
    {
        print!("\n{note}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The Vagrant provider the project's files are for: the defaults' (`vagrant.provider`), else
/// Vagrant's own `VAGRANT_DEFAULT_PROVIDER`.
fn vagrant_provider(profile: &spec::Profile, dir: &Path) -> Option<String> {
    spec::load_settings(profile, dir, None, &[])
        .ok()
        .and_then(|(_, r)| r.defaults.vagrant.provider)
        .or_else(|| std::env::var("VAGRANT_DEFAULT_PROVIDER").ok().filter(|v| !v.is_empty()))
}
