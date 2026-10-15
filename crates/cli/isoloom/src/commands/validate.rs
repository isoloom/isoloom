//! `isoloom validate`: the spec's mistakes, and the paths it mentions that don't exist.

use std::path::PathBuf;

use super::{Console, Result, Verdict, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
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

/// Lists the spec's problems: its answer is the verdict, so a spec with problems is a no, not
/// an error.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    // A folder with no spec is an error: there is nothing to give a verdict on.
    isoloom_spec::find(&a.dir)?;
    let file = spec::file_name(&a.dir);
    let profile = profile.load(&a.dir)?;
    let (name, problems) = match isoloom_spec::load_profiled(&a.dir, profile.as_ref(), &[]) {
        Ok(spec) => {
            let name = spec.name.clone();
            let lab = isoloom_targets::Lab::new(spec);
            (name, if a.skip_files { lab.problems() } else { lab.problems_with_files(&a.dir) })
        }
        Err(isoloom_spec::Error::Invalid { problems, .. }) => (String::new(), problems),
        Err(e) => return Err(e.into()),
    };
    if a.json {
        console.json(&serde_json::json!({ "ok": problems.is_empty(), "problems": problems }))?;
    } else if problems.is_empty() {
        console.line(format_args!("✓ {name} is valid"))?;
    } else {
        for p in &problems {
            console.line(format_args!("✗ {}", p.render(&file)))?;
        }
        console.line(format_args!("{} problem{}", problems.len(), if problems.len() == 1 { "" } else { "s" }))?;
    }
    Ok(Verdict::of(problems.is_empty()))
}
