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

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let file = isoloom_spec::find(&a.dir)?
        .file_name()
        .map_or_else(|| isoloom_spec::SPEC_FILE.to_owned(), |f| f.to_string_lossy().into_owned());
    let profile = profile.load(&a.dir)?;
    let (name, problems) = match isoloom_spec::load_profiled(&a.dir, profile.as_ref(), &[]) {
        Ok(spec) => {
            let name = spec.name.clone();
            let lab = isoloom_targets::Lab::new(spec);
            (name, if a.skip_files { lab.problems() } else { lab.problems_with_files(&a.dir) })
        }
        Err(isoloom_spec::LoadError::Invalid { problems, .. }) => (String::new(), problems),
        Err(e) => return Err(e.into()),
    };
    if a.json {
        let out = serde_json::json!({ "ok": problems.is_empty(), "problems": problems });
        output::print_json(&out)?;
        return Ok(output::exit_code(problems.is_empty()));
    }
    if problems.is_empty() {
        println!("✓ {name} is valid");
    } else {
        for p in &problems {
            println!("✗ {}", p.render(&file));
        }
        println!("{} problem{}", problems.len(), if problems.len() == 1 { "" } else { "s" });
    }
    Ok(output::exit_code(problems.is_empty()))
}
