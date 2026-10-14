//! `isoloom reset`: back to the environment as it came up. VMs restore the baseline snapshot
//! `run` saved; without one, and on the other targets, it is torn down and run again.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use isoloom_core as core;
use isoloom_core::Target;

use crate::Res;
use crate::commands::run::{Ask, up_or_down};
use crate::prepare::prepare;
use crate::tools::vagrant::BASELINE;

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
    if p.target == Target::Vagrant {
        let vdir = p.dir.join(core::instance::output_dir(ask.instance)).join("vagrant");
        if has_baseline(&vdir) {
            eprintln!("Restoring the baseline snapshot ({BASELINE}) of every machine");
            if restore_baseline(&vdir) {
                return Ok(ExitCode::SUCCESS);
            }
            eprintln!("note: restoring the snapshot failed; tearing down and running again");
        } else {
            eprintln!("No baseline snapshot (an older run, or ISOLOOM_NO_BASELINE): tearing down and running again");
        }
    }
    let down = up_or_down(&ask, true)?;
    if down != ExitCode::SUCCESS {
        return Ok(down);
    }
    up_or_down(&ask, false)
}

/// Whether the Vagrant environment has the baseline snapshot `run` saves.
fn has_baseline(vdir: &Path) -> bool {
    Command::new("vagrant")
        .args(["snapshot", "list"])
        .current_dir(vdir)
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == BASELINE))
}

/// Every machine back to the baseline snapshot, without provisioning again.
fn restore_baseline(vdir: &Path) -> bool {
    Command::new("vagrant")
        .args(["snapshot", "restore", "--no-provision", BASELINE])
        .current_dir(vdir)
        .status()
        .is_ok_and(|s| s.success())
}
