//! `isoloom test`: the environment's checks against a running target. Each target gets its
//! check runners (`local` for Docker, local VMs and Kubernetes, `remote` for the machines
//! reached over SSH), `execute` runs them and reads their PASS/FAIL lines, and `cloud` tests
//! cloud services from this machine.

mod cloud;
mod execute;
mod local;
mod remote;
mod runner;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_plan::checks::{self, Position};
use isoloom_spec::Target;

use isoloom_runtime::events::Scope;

use crate::prepare::prepare;
use crate::tools::vagrant::wake_controller;
use crate::{Res, output};
use runner::Runner;

#[derive(clap::Args)]
pub struct Args {
    /// The target the environment is running on (as for `run`).
    target: Option<String>,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// For the cloud targets, which cloud's module (as for `run`).
    #[arg(long)]
    cloud: Option<String>,
    /// Your own image table (YAML), as for `generate`.
    #[arg(long)]
    images: Option<PathBuf>,
    /// Only the spec's own checks; skip the derived ones.
    #[arg(long)]
    no_derived: bool,
    /// Machine-readable output: JSON events, one per line (a `check` event per result), then
    /// the outcome or the error (see docs/json.md).
    #[arg(long)]
    pub json: bool,
    /// The SSH private key for the cloud targets (default: your SSH agent and config).
    #[arg(long)]
    ssh_key: Option<PathBuf>,
    /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
    /// ports, files under .isoloom-N/. For several copies on one host.
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
    /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

/// What the runners of a test are built from.
struct Ctx<'a> {
    spec: &'a isoloom_spec::Spec,
    /// The project folder, absolute.
    dir: &'a Path,
    /// The output folder (of the instance).
    out: PathBuf,
    /// The positions checks run from, in order.
    positions: Vec<Position>,
    default_pos: Position,
    cloud: Option<&'a str>,
    ssh_key: Option<&'a Path>,
    no_derived: bool,
    instance: Option<u8>,
}

impl Ctx<'_> {
    /// The runner service (or Job) of a position.
    fn runner_name(&self, pos: &Position) -> String {
        if *pos == self.default_pos {
            "isoloom-check".to_string()
        } else {
            format!("isoloom-check-{}", pos.id())
        }
    }

    /// `ISOLOOM_DERIVED=0 ` before a runner's command with --no-derived.
    fn derived_prefix(&self) -> &'static str {
        if self.no_derived { "ISOLOOM_DERIVED=0 " } else { "" }
    }

    /// The `exec` checks' runner of a machine, under `<out>/<sub>/checks`, when it has one.
    fn exec_runner(&self, sub: &str, machine: &str) -> Option<PathBuf> {
        let file = self.out.join(sub).join("checks").join(isoloom_targets::exec_runner(machine));
        file.exists().then_some(file)
    }
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let prep = Scope::step(a.target.as_deref(), "prepare");
    output::begin(&prep, None);
    let p = prepare(&a.dir, a.target.as_deref(), a.images.as_deref(), a.instance, &a.sets)?;
    let t = p.target;
    output::done(&prep, Some(t.id().to_string()));
    if t == Target::CloudServices {
        return cloud::test(&p.spec, &p.dir, a.instance);
    }
    // The checks as the plan resolved them for the target.
    let plan = &p.plan.checks;
    if plan.iter().all(|c| a.no_derived && c.derived) {
        if output::json() {
            nothing_to_check(t);
        } else {
            println!("nothing to check: no `checks:` in the spec, and no derived checks (no services other machines could reach)");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let ctx = Ctx {
        spec: &p.spec,
        dir: &p.dir,
        out: p.dir.join(isoloom_spec::instance::output_dir(a.instance)),
        positions: checks::by_position(&p.spec, plan).into_iter().map(|(pos, _)| pos).collect(),
        default_pos: checks::default_position(&p.spec),
        cloud: a.cloud.as_deref(),
        ssh_key: a.ssh_key.as_deref(),
        no_derived: a.no_derived,
        instance: a.instance,
    };
    // A manifest written for this run (Kubernetes with --no-derived), removed at the end.
    let (runners, temp) = runners(&ctx, t)?;
    // A controller halted after provisioning runs checks again: booted for them, halted after.
    let _rehalt = match t {
        Target::Vagrant | Target::DockerVm | Target::Hybrid => wake_controller(&ctx.out, t, output::json())?,
        _ => None,
    };
    let tally = execute::run_all(&runners, t)?;
    if let Some(d) = temp {
        let _ = std::fs::remove_dir_all(d);
    }
    Ok(execute::summarize(t, &tally))
}

/// The `result` event of a test with nothing to check: success, no results.
fn nothing_to_check(t: Target) {
    output::result(
        "test",
        true,
        serde_json::json!({ "target": t.id(), "passed": 0, "failed": 0, "unfinished": [], "results": [] }),
    );
}

/// The check runners of the target, and a folder written for them to remove at the end.
fn runners(ctx: &Ctx, t: Target) -> Res<(Vec<Runner>, Option<PathBuf>)> {
    Ok(match t {
        // Handled before (cloud::test): no machines, no runners.
        Target::CloudServices => return Err("cloud services are tested by their own checks".into()),
        Target::Docker | Target::Hosted => (local::compose(ctx)?, None),
        Target::Vagrant | Target::DockerVm | Target::Hybrid => (vec![local::vm(ctx, t)], None),
        Target::Kubernetes => local::kubernetes(ctx)?,
        Target::CloudVm => (remote::cloud_vm(ctx)?, None),
        Target::CloudDocker => (remote::cloud_docker(ctx)?, None),
        Target::Proxmox => (remote::proxmox(ctx)?, None),
        Target::External => (remote::external(ctx)?, None),
    })
}
