//! The checks of a VM target: a runner script per position (`.isoloom/<dir>/checks/<position>.sh`,
//! addresses as the machines have them), and the `checks` output saying which machine runs each
//! (Proxmox, ESXi, `cloud-vm`).

use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Host, Position};
use isoloom_render::hcl_builder::{Block, Expression, list, object, string};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};

use crate::module::output;

/// The runner scripts, under `.isoloom/<dir>/checks/`. A script check runs from the project copy
/// at `/opt/isoloom`.
pub(crate) fn runners(env: &Environment, dir: &str) -> Vec<GeneratedFile> {
    let host = |h: &Host, _: &Position| -> String {
        match h {
            Host::Literal(l) => l.clone(),
            Host::Machine { name, network } => env.machines[name].addresses[network].to_string(),
        }
    };
    let run_script = |path: &str| format!("cd /opt/isoloom && sh {path}");
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    checks::by_position(env.spec(), &env.checks)
        .into_iter()
        .map(|(pos, group)| GeneratedFile {
            path: format!("{OUTPUT_DIR}/{dir}/checks/{}.sh", pos.id()),
            contents: checks::script(&pos, &group, &render),
        })
        .collect()
}

/// Where a runner runs, and as whom.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Runner {
    /// The machine (or `controller`).
    pub(crate) machine: String,
    /// Its address, from where the operator stands.
    pub(crate) host: Expression,
    /// The user to log in as.
    pub(crate) user: Expression,
}

/// How the operator runs a runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Invocation {
    /// A command on the machine, from the project copy there (`command`).
    Command,
    /// The script piped to the machine (`script`, a path from the project folder).
    Script,
}

/// The `checks` output: each position's runner, on the machine `at` says (a position it gives
/// none for is left out). `None` when no runner runs anywhere.
pub(crate) fn output_block(env: &Environment, dir: &str, how: Invocation, at: &dyn Fn(&Position) -> Option<Runner>) -> Option<Block> {
    let entries: Vec<Expression> = checks::by_position(env.spec(), &env.checks)
        .into_iter()
        .filter_map(|(pos, _)| {
            let r = at(&pos)?;
            let script = format!("{OUTPUT_DIR}/{dir}/checks/{}.sh", pos.id());
            let run = match how {
                Invocation::Command => ("command", string(format!("cd /opt/isoloom && sh {script}"))),
                Invocation::Script => ("script", string(script)),
            };
            Some(object([
                ("position", string(pos.id())),
                ("machine", string(r.machine)),
                ("host", r.host),
                ("user", r.user),
                run,
            ]))
        })
        .collect();
    (!entries.is_empty()).then(|| output("checks", list(entries)))
}
