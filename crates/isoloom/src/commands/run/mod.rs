//! `isoloom run` (and `down`, the same dispatch with the inverse command): pick the target,
//! generate its files, then bring it up (or tear it down) with the tool that owns that output.

mod bring_up;
mod deliver;
mod done;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_spec::Target;

pub use bring_up::{Invocation, Way, bring_up};

use isoloom_runtime::events::{ErrorCode, Scope};

use crate::failure::Failure;
use crate::prepare::{Prepared, prepare};
use crate::tools::{compose, process, terraform};
use crate::{Res, cloud_services, external, output, secrets, spec, target};

#[derive(clap::Args)]
pub struct Args {
    /// The target to run (docker, vagrant, docker-vm, hybrid, kubernetes, proxmox, esxi,
    /// cloud-docker, cloud-vm). Omitted, the single derived target, or an error listing them.
    target: Option<String>,
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// For `cloud-docker` and `cloud-vm`, which cloud's module to apply (aws, azure, gcp,
    /// digitalocean, linode, oci). On `cloud-vm`, omitted: AWS when it can run the environment,
    /// else the only cloud that can.
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
        volumes: false,
    };
    let up = up_or_down(&ask, false)?;
    Ok(finish("run", &ask, &up))
}

/// What `run`, `down` and `reset` were asked for.
#[derive(Clone, Copy)]
pub struct Ask<'a> {
    pub dir: &'a Path,
    pub target: Option<&'a str>,
    pub cloud: Option<&'a str>,
    pub images: Option<&'a Path>,
    pub instance: Option<u8>,
    pub sets: &'a [String],
    /// Down removes the volumes kept past it too (`down --volumes`).
    pub volumes: bool,
}

/// An environment brought up or torn down.
pub struct Brought {
    pub target: Target,
    /// The cloud it runs on (`--cloud`, or the one `cloud-vm` picked).
    pub cloud: Option<String>,
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
            "cloud": b.cloud,
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
    let cloud = cloud_for(&p, ask, down)?;
    let ask = &Ask {
        cloud: cloud.as_deref(),
        ..*ask
    };
    let mut values = Vec::new();
    if !down {
        let ready = Scope::step(Some(t.id()), "ready");
        output::begin(&ready, None);
        ready_here(&p, ask.cloud)?;
        // Every secret read before anything starts: a missing one stops the run here.
        values = secrets::read(&p)?;
        output::done(&ready, None);
    }
    if t == Target::External && !down {
        deliver::before_external(&p, ask, &values)?;
        external::external_up(dir, &p.plan, ask.instance)?;
    }
    // A project name a tool recorded for this environment stays with it.
    let project = matches!(t, Target::Docker | Target::Hosted)
        .then(|| compose::compose_project(dir, ask.instance))
        .flatten();
    let tool = launch(&p, ask, down, &values)?;
    let message = done::after(&p, ask, down, &tool, project);
    Ok(Brought {
        target: t,
        cloud: cloud.clone(),
        dir: dir.clone(),
        message,
    })
}

/// The cloud a `cloud-vm` run or teardown works on (`target::vm_cloud`); a run stops on a cloud
/// that can't run the environment, with its reason. Other targets keep `--cloud` as given.
fn cloud_for(p: &Prepared, ask: &Ask, down: bool) -> Res<Option<String>> {
    if p.target != Target::CloudVm {
        return Ok(ask.cloud.map(str::to_string));
    }
    let cloud = target::vm_cloud(&p.dir, ask.instance, &p.spec, ask.cloud)?;
    if !down && let Some(why) = target::vm_cloud_refusal(&p.spec, &cloud) {
        return Err(Failure::new(ErrorCode::TargetUnavailable, why).into());
    }
    Ok(Some(cloud))
}

/// Runs the target's tool that brings the environment up (or down): the `up` (`down`) step,
/// with the secrets handed to it.
fn launch(p: &Prepared, ask: &Ask, down: bool, values: &secrets::Values) -> Res<Invocation> {
    let t = p.target;
    let way = match (down, ask.volumes) {
        (false, _) => Way::Up,
        (true, false) => Way::Down,
        (true, true) => Way::DownWithVolumes,
    };
    let mut tool = bring_up(&p.dir, t, ask.cloud, ask.instance, way)?;
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
        // The deployment's own name, for the module to tag what it creates (kept until `down`
        // finds nothing left with it). Cloud services cost from the first resource: recorded
        // before the apply, so `status` shows them deploying (and what a failed apply left).
        cloud_services::name_deployment(&tool.wd, &p.spec.name, ask.instance)?;
        done::record(p, ask, false, None);
    }
    let mut cmd = process::command(&tool.program);
    cmd.args(&tool.args).current_dir(&tool.wd);
    if matches!(t, Target::CloudServices | Target::CloudVm | Target::CloudDocker) {
        cmd.envs(terraform::aws_session_env());
    }
    secrets::deliver(p, values, &mut cmd)?;
    if !down {
        project_files(p, &mut cmd)?;
    }
    let status = deliver::run_tool(p, ask, values, &mut cmd, &at)?;
    if !status.success() {
        return Err(Failure::new(ErrorCode::ToolFailed, format!("{} exited with {status}", tool.program)).into());
    }
    output::done(&at, None);
    Ok(tool)
}

/// The project files Compose copies in with their owner and mode (`docker`, `hosted`), read
/// from the project into the variables the Compose file names: the generated file holds none
/// of them. A Docker host VM reads them itself, from the project it unpacked.
fn project_files(p: &Prepared, cmd: &mut std::process::Command) -> Res<()> {
    if !matches!(p.target, Target::Docker | Target::Hosted) {
        return Ok(());
    }
    for (var, path) in isoloom_targets::compose_project_variables(&p.plan) {
        let text = std::fs::read_to_string(p.dir.join(&path)).map_err(|e| {
            Failure::new(ErrorCode::Io, format!("can't read the project file `{path}` a machine's `files` copy: {e}"))
                .hint("Compose copies a file with an owner or a mode from text: it must exist and be UTF-8")
        })?;
        cmd.env(var, text);
    }
    Ok(())
}

/// Whether this machine can run the target, and the Vagrant provider in the defaults can run
/// this environment.
fn ready_here(p: &Prepared, cloud: Option<&str>) -> Res<()> {
    let t = p.target;
    let own = p.spec.cloud.as_ref().map(|c| c.provider.id());
    let ready = isoloom_runtime::host::check_in(t, cloud.or(own), spec::child_env());
    if !ready.ready {
        let message = format!(
            "this machine can't run `{}` yet: {} (see `isoloom doctor`)",
            t.id(),
            ready.summary().trim_start_matches("not ready: ")
        );
        return Err(Failure::new(ErrorCode::HostNotReady, message).hint("isoloom doctor").into());
    }
    if t == Target::CloudServices {
        module_requirements(p)?;
    }
    // The provider in the defaults can refuse what the target alone runs (QEMU's one network a
    // VM, UTM's none, a machine with no Hyper-V box).
    if t == Target::Vagrant
        && let Some(provider) = &p.defaults.vagrant.provider
        && let Some(why) = isoloom_targets::refusal_on(&p.spec, t, Some(provider))
    {
        let message = format!(
            "`{}` can't run this environment on the `{provider}` provider (defaults: vagrant.provider): {why}",
            t.id()
        );
        return Err(Failure::new(ErrorCode::TargetUnavailable, message).into());
    }
    Ok(())
}

/// What the cloud-services module needs here besides Terraform (`cloud.requires`): its shell
/// commands run too, as the project's code is about to.
fn module_requirements(p: &Prepared) -> Res<()> {
    match isoloom_runtime::host::requirements(&p.spec, true).filter(|r| !r.ready) {
        Some(r) => {
            let message = format!(
                "this machine lacks what the module needs: {} (`cloud.requires`; see `isoloom doctor`)",
                r.summary().trim_start_matches("not ready: ")
            );
            Err(Failure::new(ErrorCode::HostNotReady, message).hint("isoloom doctor").into())
        }
        None => Ok(()),
    }
}
