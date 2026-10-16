//! The `vagrant` target's run manifest (`.isoloom/vagrant/run.json`): what the Isothrum runner
//! executes. It is built from the plan and the Vagrantfile's shape ([`Shape`]), never re-read
//! from the file's text, by pure functions. One manifest serves every provider: the runner picks
//! `${run.provider}` among `identity.vagrant.providers`, and steps, params and slots that only
//! concern some providers say so. Vagrant's state for the run lives in the run's state folder
//! (`identity.vagrant.dotfile`), not next to the Vagrantfile.

mod checks;
mod lifecycle;
mod machines;
mod providers;
mod slots;

use std::collections::BTreeSet;

use isoloom_manifest::{
    Argv, Checks, Driver, HostArgv, HostOs, Manifest, ManifestVersion, Part, PathTemplate, Placeholder, RelPath, RunVar, SCHEMA_URL, Template,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest;

use crate::error::Result;
use crate::vms::Shape;

/// Where Vagrant keeps the run's machines, in the run's state folder (`VAGRANT_DOTFILE_PATH`).
const DOTFILE: &str = "vagrant";

/// The vagrant target's run manifest for `env`; its message is left empty.
pub(crate) fn vagrant(env: &Environment) -> Result<Manifest> {
    let shape = Shape::of(env);
    let machines = machines::machines(env, &shape)?;
    let runners = checks::runners(&shape)?;
    let lifecycle = lifecycle::lifecycle(env, &shape)?;
    Ok(Manifest {
        target: isoloom_manifest::Target::Vagrant,
        driver: Driver::Vagrant,
        cloud: None,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: providers::tools()?,
        identity: providers::identity(env, &PathTemplate::in_state_dir(&RelPath::parse(DOTFILE)?))?,
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: run_manifest::generator()?,
        lab: run_manifest::lab(env)?,
        params: slots::params(&runners)?,
        secrets: slots::slots(env, &lifecycle)?,
        networks: run_manifest::networks(env, |n| Template::text(&format!("isoloom-{}-{n}", env.name)))?,
        outputs: run_manifest::outputs_on_their_hosts(&machines)?,
        machines,
        ready: lifecycle::ready(&shape)?,
        lifecycle,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: checks::setup(env, &shape)?,
            runners,
            teardown: checks::teardown(env, &shape)?,
        },
        recovery: Some(lifecycle::recovery()?),
        expiry: None,
        guest: None,
    })
}

/// `vagrant <args>`.
fn vagrant_argv(args: &[&str]) -> Result<HostArgv> {
    Ok(run_manifest::argv("vagrant", args)?)
}

/// `vagrant` with `args`, placeholders among them.
fn vagrant_with(args: Vec<Template>) -> Result<HostArgv> {
    Ok(HostArgv::Every(Argv::new(run_manifest::program("vagrant")?, args)))
}

/// `vagrant <args> --provider ${run.provider} <after>`.
fn on_provider(args: &[&str], after: &[&str]) -> Result<HostArgv> {
    let mut all: Vec<Template> = args.iter().map(|a| Template::text(a)).collect();
    all.push(Template::text("--provider"));
    all.push(Template::from_parts([Part::Placeholder(Placeholder::Run(RunVar::Provider))]));
    all.extend(after.iter().map(|a| Template::text(a)));
    vagrant_with(all)
}

/// Whether the controller stays on once the lab is built: then nothing halts it.
fn keeps_controller(env: &Environment) -> bool {
    env.spec().controller.as_ref().is_some_and(|c| c.keep_running)
}
