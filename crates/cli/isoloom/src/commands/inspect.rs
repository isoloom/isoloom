//! `isoloom inspect [path]`: the resolved snapshot, all of it or the part at a dotted path.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Res, output, spec};

#[derive(clap::Args)]
pub struct Args {
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
    /// Override a value for this command: `-s machines.web.vm.os=ubuntu@24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: Args, profile: &spec::Profile) -> Res<ExitCode> {
    // `isoloom inspect examples/segmented` names the project, not a path in the snapshot.
    let (what, dir) = match (a.what, a.dir) {
        (Some(w), None) if w.contains('/') || std::path::Path::new(&w).is_dir() => (None, PathBuf::from(w)),
        (w, d) => (w, d.unwrap_or_else(|| PathBuf::from("."))),
    };
    let all = spec::snapshot_of(profile, &dir, a.instance, &a.sets)?;
    let value = match &what {
        Some(path) => {
            isoloom_targets::resolved::lookup(&all, path).ok_or_else(|| format!("nothing at `{path}` in the snapshot; try `isoloom inspect` to see it all"))?
        }
        None => &all,
    };
    if a.yaml {
        print!("{}", serde_yaml_ng::to_string(value)?);
    } else {
        output::print_json(value)?;
    }
    Ok(ExitCode::SUCCESS)
}
