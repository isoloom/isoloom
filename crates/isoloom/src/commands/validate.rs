//! `isoloom validate`: the spec's mistakes, and the paths it mentions that don't exist.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    /// The project folder (holding isoloom.yml).
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Don't check that files mentioned by the spec exist.
    #[arg(long)]
    skip_files: bool,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let spec = spec::load_spec(&a.dir)?;
    let mut problems = isoloom_spec::validate(&spec);
    if !a.skip_files {
        problems.extend(isoloom_spec::validate_files(&spec, &a.dir));
    }
    if a.json {
        let list: Vec<_> = problems.iter().map(|p| serde_json::json!({ "at": p.at, "message": p.message })).collect();
        output::print_json(&serde_json::json!({ "ok": problems.is_empty(), "problems": list }))?;
    } else if problems.is_empty() {
        println!("✓ {} is valid", spec.name);
    } else {
        for p in &problems {
            println!("✗ {p}");
        }
        println!("{} problem{}", problems.len(), if problems.len() == 1 { "" } else { "s" });
    }
    Ok(output::exit_code(problems.is_empty()))
}
