//! `isoloom reset`: back to the environment as it came up. VMs restore the baseline snapshot
//! `run` saved; without one, and on the other targets, it is torn down and run again.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_core as core;
use isoloom_core::Target;

use isoloom_core::events::Scope;

use crate::commands::run::{Ask, Brought, finish, up_or_down};
use crate::prepare::prepare;
use crate::tools::process;
use crate::tools::vagrant::BASELINE;
use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    target: Option<String>,
    #[arg(default_value = ".")]
    dir: PathBuf,
    #[arg(long)]
    cloud: Option<String>,
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
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
    let p = prepare(ask.dir, ask.target, None, ask.instance, ask.sets)?;
    if p.target == Target::Vagrant && restored(&p.dir.join(core::instance::output_dir(ask.instance)).join("vagrant")) {
        let back = Brought {
            target: p.target,
            dir: p.dir,
            message: None,
        };
        return Ok(finish("reset", &ask, &back));
    }
    up_or_down(&ask, true)?;
    let up = up_or_down(&ask, false)?;
    Ok(finish("reset", &ask, &up))
}

/// Every machine back to the baseline snapshot, when there is one and restoring it works
/// (the `restore` step); false to tear down and run again.
fn restored(vdir: &Path) -> bool {
    let json = output::json();
    if !has_baseline(vdir) {
        if !json {
            eprintln!("No baseline snapshot (an older run, or ISOLOOM_NO_BASELINE): tearing down and running again");
        }
        return false;
    }
    let at = Scope::step(Some(Target::Vagrant.id()), "restore");
    output::announce(
        &at,
        Some(BASELINE.to_string()),
        &format!("Restoring the baseline snapshot ({BASELINE}) of every machine"),
    );
    let mut c = process::command("vagrant");
    c.args(["snapshot", "restore", "--no-provision", BASELINE]).current_dir(vdir);
    if process::run_step(&mut c, &at).is_ok_and(|s| s.success()) {
        output::done(&at, None);
        return true;
    }
    output::failed(
        &at,
        "restoring the snapshot failed; tearing down and running again",
        Some("note: restoring the snapshot failed; tearing down and running again"),
    );
    false
}

/// Whether the Vagrant environment has the baseline snapshot `run` saves.
fn has_baseline(vdir: &Path) -> bool {
    crate::tools::process::command("vagrant")
        .args(["snapshot", "list"])
        .current_dir(vdir)
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == BASELINE))
}
