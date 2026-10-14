//! `isoloom doctor`: what this machine can run, target by target, what the spec in the folder
//! (if any) asks of it for its cloud services (`cloud.requires`), and where each of its secrets
//! comes from and whether it can be read now.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, output, secrets, spec};

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
    let secrets = spec_secrets(&a.dir);
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
            .chain(secrets.iter().map(|s| serde_json::json!(s)))
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
    for s in &secrets {
        let at = format!("secret {}", s.name);
        let from = format!("{}: {}", s.store, s.reference);
        match &s.missing {
            None => println!("✓ {at:<22} {from}"),
            Some(why) => println!("✗ {at:<22} {from}: {why}"),
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The secrets of the spec in `dir`, if there is one, with where each comes from.
fn spec_secrets(dir: &Path) -> Vec<secrets::Source> {
    if isoloom_spec::find(dir).is_err() {
        return Vec::new();
    }
    let Ok(spec) = spec::load_spec(dir) else { return Vec::new() };
    secrets::sources(&spec, dir)
}

/// What the cloud-services module of the spec in `dir` needs on this machine (`cloud.requires`):
/// command names only; its shell commands are the project's code, run by `run` alone.
fn required(dir: &Path) -> Option<isoloom_runtime::host::Readiness> {
    isoloom_spec::find(dir).ok()?;
    let spec = spec::load_spec(dir).ok()?;
    isoloom_runtime::host::requirements(&spec, false)
}
