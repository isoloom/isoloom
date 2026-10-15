//! `isoloom profiles`: each of the project's profiles, checked against the spec.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    json: bool,
}

/// A profile as listed: its name, target, description and problems.
struct Checked {
    name: String,
    target: Option<&'static str>,
    description: Option<String>,
    problems: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let dir = &a.dir;
    let out: Vec<Checked> = isoloom_spec::profile::list(dir).iter().map(|name| check(dir, name)).collect();
    let failed = out.iter().any(|c| !c.problems.is_empty());
    if a.json {
        let items: Vec<serde_json::Value> = out
            .iter()
            .map(|c| serde_json::json!({ "name": c.name, "target": c.target, "description": c.description, "ok": c.problems.is_empty(), "problems": c.problems }))
            .collect();
        output::print_json(&items)?;
    } else if out.is_empty() {
        println!(
            "no profiles in {} (one file per customer or site: profiles/<name>.yml)",
            dir.join(isoloom_spec::profile::DIR).display()
        );
    } else {
        let width = out.iter().map(|c| c.name.len()).max().unwrap_or(0);
        for c in &out {
            let mark = if c.problems.is_empty() { '✓' } else { '✗' };
            println!(
                "{mark} {:<width$}  {:<14}  {}",
                c.name,
                c.target.unwrap_or("any target"),
                c.description.as_deref().unwrap_or("")
            );
            for problem in &c.problems {
                println!("    {problem}");
            }
        }
    }
    Ok(output::exit_code(!failed))
}

/// A profile loaded, the spec it makes validated, and its target checked against that spec.
fn check(dir: &Path, name: &str) -> Checked {
    let profile = isoloom_spec::profile::load(dir, name);
    let (target, description) = match &profile {
        Ok(p) => (p.target.map(isoloom_spec::Target::id), p.description.clone()),
        Err(_) => (None, None),
    };
    let problems = match profile {
        Err(e) => vec![e],
        Ok(p) => problems_of(dir, &p),
    };
    Checked {
        name: name.to_string(),
        target,
        description,
        problems,
    }
}

fn problems_of(dir: &Path, p: &isoloom_spec::profile::Profile) -> Vec<String> {
    let spec = match isoloom_spec::load_profiled(dir, Some(p), &[]) {
        Err(isoloom_spec::LoadError::Invalid { problems, .. }) => return problems.iter().map(|e| format!("the spec it makes: {}", e.line())).collect(),
        Err(e) => return e.to_string().lines().map(str::to_string).collect(),
        Ok(spec) => spec,
    };
    let lab = match isoloom_spec::defaults::load_profiled(dir, None, Some(p), &[]) {
        Err(e) => return vec![e],
        Ok(r) => isoloom_targets::Lab::new(spec).with_images(r.defaults.images),
    };
    let mut problems: Vec<String> = lab.problems().iter().map(|e| format!("the spec it makes: {}", e.line())).collect();
    if let Some(t) = p.target
        && problems.is_empty()
        && !lab.possible().contains(&t)
    {
        let why = isoloom_targets::refusal(&lab, t).unwrap_or_else(|| "not one of its targets".into());
        problems.push(format!("can't run on `{}`: {why}", t.id()));
    }
    problems
}
