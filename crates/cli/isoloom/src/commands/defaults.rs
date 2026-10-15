//! `isoloom defaults`: the defaults in effect here, and where each comes from.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Only the built-in layer.
    #[arg(long)]
    system: bool,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    if !a.system {
        return in_effect(profile, &a.dir, a.json);
    }
    let value = isoloom_spec::defaults::builtin();
    if a.json {
        output::print_json(&serde_json::to_value(&value)?)?;
    } else {
        print!("{}", serde_yaml_ng::to_string(&value)?);
    }
    Ok(ExitCode::SUCCESS)
}

/// Every layer merged, each value with its source.
fn in_effect(profile: &spec::Profile, dir: &Path, json: bool) -> Res<ExitCode> {
    let profile = profile.load(dir)?;
    let r = isoloom_spec::defaults::load_profiled(dir, None, profile.as_ref(), &[])?;
    let mut merged = isoloom_spec::defaults::builtin();
    isoloom_spec::defaults::merge(&mut merged, &r.merged);
    if json {
        let sources: serde_json::Map<String, serde_json::Value> = r.sources.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        output::print_json(
            &serde_json::json!({ "defaults": serde_json::to_value(&merged)?, "sources": sources, "user_file": isoloom_spec::defaults::user_file(), "project_file": dir.join(isoloom_spec::defaults::PROJECT_FILE) }),
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    print_sources(&merged, &r.sources);
    println!(
        "\nfiles: {} (yours), {} (the project's); environment ISOLOOM_<KEY>; -s defaults.<key>=<value>",
        isoloom_spec::defaults::user_file().display(),
        dir.join(isoloom_spec::defaults::PROJECT_FILE).display()
    );
    Ok(ExitCode::SUCCESS)
}

/// One line per value: its key, the value and where it comes from.
fn print_sources(merged: &serde_yaml_ng::Value, sources: &[(String, String)]) {
    let builtin_leaves = isoloom_spec::defaults::leaves(&isoloom_spec::defaults::builtin(), "");
    let width = sources
        .iter()
        .map(|(k, _)| k.len())
        .chain(builtin_leaves.iter().map(String::len))
        .max()
        .unwrap_or(0);
    for leaf in isoloom_spec::defaults::leaves(merged, "") {
        let value = leaf
            .split('.')
            .try_fold(merged, |v, k| v.get(k))
            .map_or_else(String::new, |v| serde_yaml_ng::to_string(v).unwrap_or_default().trim().to_string());
        let source = sources.iter().find(|(k, _)| *k == leaf).map_or("built in", |(_, s)| s.as_str());
        println!("{leaf:<width$}  {value:<24}  {source}");
    }
}
