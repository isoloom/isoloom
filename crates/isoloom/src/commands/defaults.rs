//! `isoloom defaults`: the defaults in effect here, and where each comes from.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_core as core;

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

pub fn run(a: &Args) -> Res<ExitCode> {
    if !a.system {
        return in_effect(&a.dir, a.json);
    }
    let value = core::defaults::builtin();
    if a.json {
        output::print_json(&serde_json::to_value(&value)?)?;
    } else {
        print!("{}", serde_yaml_ng::to_string(&value)?);
    }
    Ok(ExitCode::SUCCESS)
}

/// Every layer merged, each value with its source.
fn in_effect(dir: &Path, json: bool) -> Res<ExitCode> {
    let profile = spec::profile_for(dir)?;
    let r = core::defaults::load_profiled(dir, None, profile.as_ref(), &[])?;
    let mut merged = core::defaults::builtin();
    core::defaults::merge(&mut merged, &r.merged);
    if json {
        let sources: serde_json::Map<String, serde_json::Value> = r.sources.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        output::print_json(
            &serde_json::json!({ "defaults": serde_json::to_value(&merged)?, "sources": sources, "user_file": core::defaults::user_file(), "project_file": dir.join(core::defaults::PROJECT_FILE) }),
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    print_sources(&merged, &r.sources);
    println!(
        "\nfiles: {} (yours), {} (the project's); environment ISOLOOM_<KEY>; -s defaults.<key>=<value>",
        core::defaults::user_file().display(),
        dir.join(core::defaults::PROJECT_FILE).display()
    );
    Ok(ExitCode::SUCCESS)
}

/// One line per value: its key, the value and where it comes from.
fn print_sources(merged: &serde_yaml_ng::Value, sources: &[(String, String)]) {
    let builtin_leaves = core::defaults::leaves(&core::defaults::builtin(), "");
    let width = sources
        .iter()
        .map(|(k, _)| k.len())
        .chain(builtin_leaves.iter().map(String::len))
        .max()
        .unwrap_or(0);
    for leaf in core::defaults::leaves(merged, "") {
        let value = leaf
            .split('.')
            .try_fold(merged, |v, k| v.get(k))
            .map(|v| serde_yaml_ng::to_string(v).unwrap_or_default().trim().to_string())
            .unwrap_or_default();
        let source = sources.iter().find(|(k, _)| *k == leaf).map(|(_, s)| s.as_str()).unwrap_or("built in");
        println!("{leaf:<width$}  {value:<24}  {source}");
    }
}
