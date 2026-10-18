//! `isoloom message`: the spec's `message` (how to use the environment), its placeholders filled.

use super::{Console, Project, Result, Verdict};
use crate::{spec, target};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: Project,
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

/// Prints the message as it reads where the environment runs.
pub(crate) fn run(a: &Args, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let lab = spec::valid(profile, &a.project.dir, a.instance, &a.sets)?;
    // Where it runs decides the addresses (Docker blocks on the Compose targets).
    let on = match a.target.as_deref() {
        Some(id) => Some(target::by_id(id)?),
        None => match lab.possible().as_slice() {
            [one] => Some(*one),
            _ => None,
        },
    };
    let message = match on {
        Some(t) => isoloom_targets::resolved::render_message_on(&lab, a.instance, t)?,
        None => isoloom_targets::resolved::render_message(&lab, a.instance)?,
    };
    match message {
        Some(m) => console.line(m.trim_end())?,
        None => console.note("the spec has no `message:`\n")?,
    }
    Ok(Verdict::Pass)
}
