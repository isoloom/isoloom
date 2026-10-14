//! `isoloom message`: the spec's `message` (how to use the environment), its placeholders filled.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::tools::compose;
use crate::{Res, spec, target};

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The message of instance N.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// The target it runs on, for the addresses machines have there (default: what `run`
    /// recorded for the folder, else the spec's single target, else the spec's addresses).
    #[arg(long)]
    target: Option<String>,
    /// Overrides, as for `generate`.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let spec = spec::load_valid(&a.dir, a.instance, &a.sets)?;
    // Where it runs decides the addresses (Docker blocks on the Compose targets); a named
    // target that doesn't exist is an error, an unknown one falls back to the spec's.
    let on = match target::pick(&spec::abs(&a.dir)?, a.target.as_deref(), a.instance, &spec) {
        Ok((t, _)) => Some(t),
        Err(e) if a.target.is_some() => return Err(e),
        Err(_) => None,
    };
    let message = match on {
        Some(t) => {
            let publishes = spec.machines.values().any(|m| m.ports.iter().any(|s| s.publish.is_some()));
            let ports = compose::real_ports(&spec::abs(&a.dir)?, t, a.instance, publishes);
            isoloom_targets::resolved::render_message_at(&spec, a.instance, t, &ports)?
        }
        None => isoloom_targets::resolved::render_message(&spec, a.instance)?,
    };
    match message {
        Some(m) => println!("{}", m.trim_end()),
        None => eprintln!("the spec has no `message:`"),
    }
    Ok(ExitCode::SUCCESS)
}
