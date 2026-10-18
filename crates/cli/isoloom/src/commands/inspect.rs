//! `isoloom inspect [path]`: the resolved snapshot, all of it or the part at a dotted path.

use std::path::{Path, PathBuf};

use super::{CliError, Console, Result, Verdict, spec};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// A dotted path into the snapshot (default: all of it). A folder here means the project.
    what: Option<String>,
    /// The project folder (holding isoloom.yml).
    dir: Option<PathBuf>,
    /// YAML instead of JSON.
    #[arg(long)]
    yaml: bool,
    /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
    /// ports, files under .isoloom-N/. For several copies on one host.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Override a value for this command: `-s machines.web.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// The dotted path asked for and the project folder: `isoloom inspect examples/segmented` names
/// the project, not a path in the snapshot.
fn path_and_dir(what: Option<String>, dir: Option<PathBuf>) -> (Option<String>, PathBuf) {
    match (what, dir) {
        (Some(w), None) if w.contains('/') || Path::new(&w).is_dir() => (None, PathBuf::from(w)),
        (w, d) => (w, d.unwrap_or_else(|| PathBuf::from("."))),
    }
}

/// Prints the snapshot, or the part of it at the path asked for.
pub(crate) fn run(a: Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let (what, dir) = path_and_dir(a.what, a.dir);
    let all = spec::snapshot(profile, &dir, a.instance, &a.sets)?;
    let value = match &what {
        Some(path) => isoloom_targets::resolved::lookup(&all, path).ok_or_else(|| CliError::NothingAt(path.clone()))?,
        None => &all,
    };
    if a.yaml {
        console.print(serde_yaml_ng::to_string(value)?)?;
    } else {
        console.json(value)?;
    }
    Ok(Verdict::Pass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_alone_is_the_project_and_a_dotted_path_the_part_asked_for() {
        assert_eq!(
            path_and_dir(Some("examples/segmented".into()), None),
            (None, PathBuf::from("examples/segmented"))
        );
        assert_eq!(
            path_and_dir(Some("machines.web".into()), None),
            (Some("machines.web".to_owned()), PathBuf::from("."))
        );
        assert_eq!(
            path_and_dir(Some("targets".into()), Some("lab".into())),
            (Some("targets".to_owned()), PathBuf::from("lab"))
        );
    }
}
