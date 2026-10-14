//! `isoloom run` (and `down`, the same dispatch with the inverse command): pick the target,
//! generate its files, then bring it up (or tear it down) with the tool that owns that output.

mod bring_up;
mod done;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use isoloom_core as core;
use isoloom_core::Target;

pub use bring_up::{Invocation, bring_up};

use crate::prepare::{Prepared, prepare};
use crate::tools::{compose, terraform};
use crate::{Res, cloud_services, external};

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
    up_or_down(&ask, false)
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

/// Brings the environment up, or tears it down when `down`.
pub fn up_or_down(ask: &Ask, down: bool) -> Res<ExitCode> {
    let p = prepare(ask.dir, ask.target, ask.images, ask.instance, ask.sets)?;
    let (t, dir) = (p.target, &p.dir);
    if !down {
        ready_here(&p, ask.cloud)?;
    }
    if t == Target::External && !down {
        external::external_up(dir, &p.spec, ask.instance)?;
    }
    // A project name a tool recorded for this environment stays with it.
    let project = matches!(t, Target::Docker | Target::Hosted)
        .then(|| compose::compose_project(dir, ask.instance))
        .flatten();
    let mut tool = bring_up(dir, t, ask.cloud, ask.instance, down)?;
    // The preferred Vagrant provider, from the defaults.
    if !down
        && tool.program == "vagrant"
        && let Some(p) = &p.defaults.vagrant.provider
    {
        tool.args.extend(["--provider".to_string(), p.clone()]);
    }
    eprintln!("{} {} ({})", if down { "Tearing down" } else { "Running" }, t.id(), tool.wd.display());
    if t == Target::CloudServices
        && !down
        && let Some(c) = &p.spec.cloud
    {
        cloud_services::write_inputs(c, &tool.wd)?;
    }
    let mut cmd = Command::new(&tool.program);
    cmd.args(&tool.args).current_dir(&tool.wd);
    if matches!(t, Target::CloudServices | Target::CloudVm | Target::CloudDocker) {
        cmd.envs(terraform::aws_session_env());
    }
    let program = &tool.program;
    match cmd.status() {
        Ok(s) if s.success() => {
            done::after(&p, ask, down, &tool, project);
            Ok(ExitCode::SUCCESS)
        }
        Ok(s) => Err(format!("{program} exited with {s}").into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("`{program}` isn't installed").into()),
        Err(e) => Err(Box::new(e)),
    }
}

/// Whether this machine can run the target, and the Vagrant provider in the defaults can run
/// this environment.
fn ready_here(p: &Prepared, cloud: Option<&str>) -> Res<()> {
    let t = p.target;
    let own = p.spec.cloud.as_ref().map(|c| c.provider.id());
    let ready = core::host::check(t, cloud.or(own).or((t == Target::CloudVm).then_some("aws")));
    if !ready.ready {
        return Err(format!(
            "this machine can't run `{}` yet: {} (see `isoloom doctor`)",
            t.id(),
            ready.summary().trim_start_matches("not ready: ")
        )
        .into());
    }
    // The provider in the defaults can refuse what the target alone runs (QEMU's two-VM links).
    if t == Target::Vagrant
        && let Some(provider) = &p.defaults.vagrant.provider
        && let Some(why) = core::refusal_on(&p.spec, t, Some(provider))
    {
        return Err(format!(
            "`{}` can't run this environment on the `{provider}` provider (defaults: vagrant.provider): {why}",
            t.id()
        )
        .into());
    }
    Ok(())
}
