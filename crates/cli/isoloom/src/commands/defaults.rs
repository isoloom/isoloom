//! `isoloom defaults`: the defaults in effect here, and where each comes from.

use std::path::Path;

use super::{CliError, Console, Project, Result, Verdict, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
    /// Only the built-in layer.
    #[arg(long)]
    system: bool,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
}

/// Prints the built-in defaults, or every layer merged with each value's source.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    if !a.system {
        return in_effect(profile, &a.project.dir, a.json, console);
    }
    let value = isoloom_spec::defaults::builtin();
    if a.json {
        console.json(&serde_json::to_value(&value)?)?;
    } else {
        console.print(serde_yaml_ng::to_string(&value)?)?;
    }
    Ok(Verdict::Pass)
}

/// Every layer merged, each value with its source.
fn in_effect(profile: &spec::Profile, dir: &Path, json: bool, console: &mut Console) -> Result<Verdict> {
    let profile = profile.load(dir)?;
    let host = spec::host();
    let r = isoloom_spec::defaults::load(&spec::defaults_of(dir, None, profile.as_ref(), &[])).map_err(CliError::Defaults)?;
    let mut merged = isoloom_spec::defaults::builtin();
    isoloom_spec::defaults::merge(&mut merged, &r.merged);
    let project_file = dir.join(isoloom_spec::defaults::PROJECT_FILE);
    if json {
        let sources: serde_json::Map<String, serde_json::Value> = r.sources.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        let out = serde_json::json!({ "defaults": serde_json::to_value(&merged)?, "sources": sources, "user_file": isoloom_spec::defaults::user_file(&host), "project_file": project_file });
        console.json(&out)?;
        return Ok(Verdict::Pass);
    }
    for line in source_lines(&merged, &r.sources) {
        console.line(line)?;
    }
    console.line(format_args!(
        "\nfiles: {} (yours), {} (the project's); environment ISOLOOM_<KEY>; -s defaults.<key>=<value>",
        isoloom_spec::defaults::user_file(&host).display(),
        project_file.display()
    ))?;
    Ok(Verdict::Pass)
}

/// One line per value: its key, the value and where it comes from, in aligned columns.
fn source_lines(merged: &serde_yaml_ng::Value, sources: &[(String, String)]) -> Vec<String> {
    let builtin_leaves = isoloom_spec::defaults::leaves(&isoloom_spec::defaults::builtin(), "");
    let width = sources
        .iter()
        .map(|(k, _)| k.len())
        .chain(builtin_leaves.iter().map(String::len))
        .max()
        .unwrap_or(0);
    isoloom_spec::defaults::leaves(merged, "")
        .into_iter()
        .map(|leaf| {
            let value = leaf
                .split('.')
                .try_fold(merged, |v, k| v.get(k))
                .and_then(|v| serde_yaml_ng::to_string(v).ok())
                .map(|text| text.trim().to_owned())
                .unwrap_or_default();
            let source = sources.iter().find(|(k, _)| *k == leaf).map_or("built in", |(_, s)| s.as_str());
            format!("{leaf:<width$}  {value:<24}  {source}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_value_is_listed_with_where_it_comes_from() -> std::result::Result<(), serde_yaml_ng::Error> {
        let merged: serde_yaml_ng::Value = serde_yaml_ng::from_str("vagrant: { provider: libvirt }")?;
        let lines = source_lines(&merged, &[("vagrant.provider".to_owned(), "the project's".to_owned())]);
        let line = lines
            .iter()
            .find(|l| l.starts_with("vagrant.provider"))
            .map(|l| l.split_whitespace().collect::<Vec<_>>());
        assert_eq!(line, Some(vec!["vagrant.provider", "libvirt", "the", "project's"]));
        Ok(())
    }
}
