//! `isoloom down`: what `run` started, torn down with the inverse tool.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::Res;
use crate::commands::run::{Ask, finish, up_or_down};

#[derive(clap::Args)]
pub struct Args {
    target: Option<String>,
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    cloud: Option<String>,
    /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
    /// ports, files under .isoloom-N/. For several copies on one host.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
    /// Machine-readable output: JSON events, one per line, then the outcome or the error (see
    /// docs/json.md).
    #[arg(long)]
    pub json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let ask = Ask {
        dir: &a.dir,
        target: a.target.as_deref(),
        cloud: a.cloud.as_deref(),
        images: None,
        instance: a.instance,
        sets: &a.sets,
    };
    let down = up_or_down(&ask, true)?;
    Ok(finish("down", &ask, &down))
}
