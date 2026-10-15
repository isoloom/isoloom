//! `isoloom message`: the spec's `message` (how to use the environment), its placeholders filled.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Res, spec, target};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The message of instance N.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// The target it runs on, for the addresses machines have there (default: the spec's
    /// single target, else the spec's addresses).
    #[arg(long)]
    target: Option<String>,
    /// Overrides, as for `generate`.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args, profile: &spec::Profile) -> Res<ExitCode> {
    let spec = spec::load_valid(profile, &a.dir, a.instance, &a.sets)?;
    // Where it runs decides the addresses (Docker blocks on the Compose targets).
    let on = match a.target.as_deref() {
        Some(id) => Some(target::by_id(id)?),
        None => match isoloom_spec::effective(&spec).as_slice() {
            [one] => Some(*one),
            _ => None,
        },
    };
    let message = match on {
        Some(t) => isoloom_targets::resolved::render_message_on(&spec, a.instance, t)?,
        None => isoloom_targets::resolved::render_message(&spec, a.instance)?,
    };
    match message {
        Some(m) => println!("{}", m.trim_end()),
        None => eprintln!("the spec has no `message:`"),
    }
    Ok(ExitCode::SUCCESS)
}
