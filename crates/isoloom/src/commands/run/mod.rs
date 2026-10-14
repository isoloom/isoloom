//! `isoloom run` (and `down`, the same dispatch with the inverse command): pick the target,
//! generate its files, then bring it up (or tear it down) with the tool that owns that output.

mod bring_up;
mod done;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_core as core;
use isoloom_core::Target;

pub use bring_up::{Invocation, bring_up};

use isoloom_core::events::{ErrorCode, Scope};

use crate::failure::Failure;
use crate::prepare::{Prepared, prepare};
use crate::tools::{compose, process, terraform};
use crate::{Res, cloud_services, external, output, spec};

#[derive(clap::Args)]
pub struct Args {
    /// The target to run (docker, vagrant, docker-vm, hybrid, kubernetes, proxmox,
    /// cloud-docker, cloud-vm). Omitted, the single derived target, or an error listing them.
    target: Option<String>,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// For `cloud-docker`, which cloud's module to apply (aws, azure, gcp, digitalocean,
    /// linode, oci).
    #[arg(long)]
    cloud: Option<String>,
    /// Your own image table (YAML), as for `generate`.
    #[arg(long, value_name = "FILE")]
    images: Option<PathBuf>,
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
        images: a.images.as_deref(),
        instance: a.instance,
        sets: &a.sets,
    };
    let up = up_or_down(&ask, false)?;
    Ok(finish("run", &ask, &up))
}

/// What `run`, `down` and `reset` were asked for.
pub struct Ask<'a> {
    pub dir: &'a Path,
    pub target: Option<&'a str>,
    pub cloud: Option<&'a str>,
    pub images: Option<&'a Path>,
    pub instance: Option<u8>,
    pub sets: &'a [String],
}

/// An environment brought up or torn down.
pub struct Brought {
    pub target: Target,
    /// The project folder, absolute.
    pub dir: PathBuf,
    /// The spec's message, filled in (after a run).
    pub message: Option<String>,
}

/// The `result` event of `run`, `down` and `reset` (with `--json`); the command's exit code.
pub fn finish(command: &str, ask: &Ask, b: &Brought) -> ExitCode {
    output::result(
        command,
        true,
        serde_json::json!({
            "target": b.target.id(),
            "cloud": ask.cloud,
            "instance": ask.instance,
            "dir": b.dir,
            "message": b.message,
        }),
    );
    ExitCode::SUCCESS
}

/// Brings the environment up, or tears it down when `down`.
pub fn up_or_down(ask: &Ask, down: bool) -> Res<Brought> {
    let prep = Scope::step(ask.target, "prepare");
    output::begin(&prep, None);
    let p = prepare(ask.dir, ask.target, ask.images, ask.instance, ask.sets)?;
    output::done(&prep, Some(p.target.id().to_string()));
    let (t, dir) = (p.target, &p.dir);
    if !down {
        let ready = Scope::step(Some(t.id()), "ready");
        output::begin(&ready, None);
        ready_here(&p, ask.cloud)?;
        output::done(&ready, None);
    }
    if t == Target::External && !down {
        external::external_up(dir, &p.plan, ask.instance)?;
    }
    // A project name a tool recorded for this environment stays with it.
    let project = matches!(t, Target::Docker | Target::Hosted)
        .then(|| compose::compose_project(dir, ask.instance))
        .flatten();
    let tool = launch(&p, ask, down)?;
    let message = done::after(&p, ask, down, &tool, project);
    Ok(Brought {
        target: t,
        dir: dir.clone(),
        message,
    })
}

/// Runs the target's tool that brings the environment up (or down): the `up` (`down`) step.
fn launch(p: &Prepared, ask: &Ask, down: bool) -> Res<Invocation> {
    let t = p.target;
    let mut tool = bring_up(&p.dir, t, ask.cloud, ask.instance, down)?;
    // The preferred Vagrant provider, from the defaults.
    if !down
        && tool.program == "vagrant"
        && let Some(provider) = &p.defaults.vagrant.provider
    {
        tool.args.extend(["--provider".to_string(), provider.clone()]);
    }
    let at = Scope::step(Some(t.id()), if down { "down" } else { "up" });
    let line = format!("{} {} ({})", if down { "Tearing down" } else { "Running" }, t.id(), tool.wd.display());
    output::announce(&at, Some(tool.wd.display().to_string()), &line);
    if t == Target::CloudServices
        && !down
        && let Some(c) = &p.spec.cloud
    {
        cloud_services::write_inputs(c, &tool.wd)?;
    }
    let mut cmd = process::command(&tool.program);
    cmd.args(&tool.args).current_dir(&tool.wd);
    if matches!(t, Target::CloudServices | Target::CloudVm | Target::CloudDocker) {
        cmd.envs(terraform::aws_session_env());
    }
    let status = process::run_step(&mut cmd, &at)?;
    if !status.success() {
        return Err(Failure::new(ErrorCode::ToolFailed, format!("{} exited with {status}", tool.program)).into());
    }
    output::done(&at, None);
    Ok(tool)
}

/// Whether this machine can run the target, and the Vagrant provider in the defaults can run
/// this environment.
fn ready_here(p: &Prepared, cloud: Option<&str>) -> Res<()> {
    let t = p.target;
    let own = p.spec.cloud.as_ref().map(|c| c.provider.id());
    let ready = core::host::check_in(t, cloud.or(own).or((t == Target::CloudVm).then_some("aws")), spec::child_env());
    if !ready.ready {
        let message = format!(
            "this machine can't run `{}` yet: {} (see `isoloom doctor`)",
            t.id(),
            ready.summary().trim_start_matches("not ready: ")
        );
        return Err(Failure::new(ErrorCode::HostNotReady, message).hint("isoloom doctor").into());
    }
    // The provider in the defaults can refuse what the target alone runs (QEMU's two-VM links).
    if t == Target::Vagrant
        && let Some(provider) = &p.defaults.vagrant.provider
        && let Some(why) = core::refusal_on(&p.spec, t, Some(provider))
    {
        let message = format!(
            "`{}` can't run this environment on the `{provider}` provider (defaults: vagrant.provider): {why}",
            t.id()
        );
        return Err(Failure::new(ErrorCode::TargetUnavailable, message).into());
    }
    Ok(())
}
