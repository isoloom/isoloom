//! `isoloom doctor`: what this machine can run, target by target.

use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let all = core::host::all();
    if a.json {
        let list: Vec<serde_json::Value> = all
            .iter()
            .map(|r| serde_json::json!({ "target": r.target.id(), "cloud": r.cloud, "ready": r.ready, "notes": r.notes }))
            .collect();
        output::print_json(&list)?;
        return Ok(ExitCode::SUCCESS);
    }
    for r in &all {
        let name = match &r.cloud {
            Some(c) => format!("{} ({c})", r.target.id()),
            None => r.target.id().to_string(),
        };
        println!("{} {name:<22} {}", if r.ready { "✓" } else { "✗" }, r.summary());
    }
    Ok(ExitCode::SUCCESS)
}
