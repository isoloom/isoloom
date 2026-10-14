//! `isoloom doctor`: what this machine can run, target by target, and what the spec in the
//! folder (if any) asks of it for its cloud services (`cloud.requires`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let mut all = isoloom_runtime::host::all();
    let required_at = all.len();
    all.extend(required(&a.dir));
    if a.json {
        let list: Vec<serde_json::Value> = all
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut v = serde_json::json!({ "target": r.target.id(), "cloud": r.cloud, "ready": r.ready, "notes": r.notes });
                if i >= required_at {
                    v["requires"] = true.into();
                }
                v
            })
            .collect();
        output::print_json(&list)?;
        return Ok(ExitCode::SUCCESS);
    }
    for (i, r) in all.iter().enumerate() {
        let name = match &r.cloud {
            _ if i >= required_at => "the spec's requires".to_string(),
            Some(c) => format!("{} ({c})", r.target.id()),
            None => r.target.id().to_string(),
        };
        println!("{} {name:<22} {}", if r.ready { "✓" } else { "✗" }, r.summary());
    }
    Ok(ExitCode::SUCCESS)
}

/// What the cloud-services module of the spec in `dir` needs on this machine (`cloud.requires`):
/// command names only; its shell commands are the project's code, run by `run` alone.
fn required(dir: &Path) -> Option<isoloom_runtime::host::Readiness> {
    isoloom_spec::find(dir).ok()?;
    let spec = spec::load_spec(dir).ok()?;
    isoloom_runtime::host::requirements(&spec, false)
}
