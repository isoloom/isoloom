//! `isoloom report <name>`: a table about the environment, as text or Markdown.

use std::path::Path;

use super::{Console, Project, Result, Verdict, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The report.
    name: String,
    #[command(flatten)]
    project: Project,
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

/// Prints the report asked for (`list` names them all).
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    if a.name == "list" {
        for (n, what) in isoloom_targets::report::REPORTS {
            console.line(format_args!("{n:<12} {what}"))?;
        }
        return Ok(Verdict::Pass);
    }
    let snapshot = spec::snapshot(profile, &a.project.dir, a.instance, &a.sets)?;
    console.print(isoloom_targets::report::report(&a.name, &snapshot, a.md)?)?;
    // Published ports on Hyper-V (the provider the defaults or Vagrant pick): not forwarded.
    if matches!(a.name.as_str(), "ports" | "services")
        && vagrant_provider(profile, &a.project.dir).as_deref() == Some("hyperv")
        && let Some(note) = isoloom_targets::report::hyperv_ports_note(&snapshot)
    {
        console.print(format_args!("\n{note}"))?;
    }
    Ok(Verdict::Pass)
}

/// The Vagrant provider the project's files are for: the defaults' (`vagrant.provider`), else
/// Vagrant's own `VAGRANT_DEFAULT_PROVIDER`.
fn vagrant_provider(profile: &spec::Profile, dir: &Path) -> Option<String> {
    spec::with_settings(profile, dir, None, &[])
        .ok()
        .and_then(|(_, r)| r.defaults.vagrant.provider)
        .or_else(|| std::env::var("VAGRANT_DEFAULT_PROVIDER").ok().filter(|v| !v.is_empty()))
}
