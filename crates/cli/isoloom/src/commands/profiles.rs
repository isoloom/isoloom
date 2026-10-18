//! `isoloom profiles`: each of the project's profiles, checked against the spec.

use super::{Console, Listing, Result, Verdict};
use std::path::Path;

/// A profile as listed: its name, target, description and problems.
#[derive(Debug)]
struct Checked {
    name: String,
    target: Option<&'static str>,
    description: Option<String>,
    problems: Vec<String>,
}

/// Lists each profile with its problems; a no when one has any.
pub(crate) fn run(a: &Listing, console: &mut Console) -> Result<Verdict> {
    let dir = &a.project.dir;
    let out: Vec<Checked> = isoloom_spec::profile::list(dir).iter().map(|name| check(dir, name)).collect();
    let failed = out.iter().any(|c| !c.problems.is_empty());
    if a.json {
        let items: Vec<serde_json::Value> = out
            .iter()
            .map(|c| serde_json::json!({ "name": c.name, "target": c.target, "description": c.description, "ok": c.problems.is_empty(), "problems": c.problems }))
            .collect();
        console.json(&items)?;
    } else if out.is_empty() {
        console.line(format_args!(
            "no profiles in {} (one file per customer or site: profiles/<name>.yml)",
            dir.join(isoloom_spec::profile::DIR).display()
        ))?;
    } else {
        let width = out.iter().map(|c| c.name.len()).max().unwrap_or(0);
        for c in &out {
            let mark = if c.problems.is_empty() { '✓' } else { '✗' };
            let target = c.target.unwrap_or("any target");
            console.line(format_args!(
                "{mark} {:<width$}  {target:<14}  {}",
                c.name,
                c.description.as_deref().unwrap_or("")
            ))?;
            for problem in &c.problems {
                console.line(format_args!("    {problem}"))?;
            }
        }
    }
    Ok(Verdict::of(!failed))
}

/// A profile loaded, the spec it makes checked, and its target checked against that spec.
fn check(dir: &Path, name: &str) -> Checked {
    let profile = isoloom_spec::profile::load(dir, name);
    let (target, description) = match &profile {
        Ok(p) => (p.target.map(isoloom_spec::Target::id), p.description.clone()),
        Err(_) => (None, None),
    };
    let problems = match profile {
        Err(e) => vec![e.to_string()],
        Ok(p) => problems_of(dir, &p),
    };
    Checked {
        name: name.to_owned(),
        target,
        description,
        problems,
    }
}

/// What is wrong with the spec a profile makes, and with the target it pins.
fn problems_of(dir: &Path, p: &isoloom_spec::profile::Profile) -> Vec<String> {
    let spec = match isoloom_spec::load_profiled(dir, Some(p), &[]) {
        Err(isoloom_spec::Error::Invalid { problems, .. }) => return problems.iter().map(|e| format!("the spec it makes: {}", e.line())).collect(),
        Err(e) => return e.to_string().lines().map(str::to_owned).collect(),
        Ok(spec) => spec,
    };
    let lab = match isoloom_spec::defaults::load(&crate::spec::defaults_of(dir, None, Some(p), &[])) {
        Err(e) => return vec![e.to_string()],
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
