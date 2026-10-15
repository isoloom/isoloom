//! `isoloom resources`: the machines, CPUs, memory and disk the spec needs.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let spec = spec::load_spec(profile, &a.dir)?;
    let t = isoloom_spec::totals(&spec);
    if a.json {
        output::print_json(&serde_json::json!({ "machines": t.machines, "cpus": t.cpus, "memoryMb": t.memory_mb, "diskGb": t.disk_gb }))?;
    } else {
        println!(
            "{} machines · {} CPUs · {:.1} GB memory · {} GB disk (as VMs)",
            t.machines,
            t.cpus,
            f64::from(t.memory_mb) / 1024.0,
            t.disk_gb
        );
    }
    Ok(ExitCode::SUCCESS)
}
