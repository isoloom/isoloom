//! The `docker` target's run manifest (`.isoloom/docker/run.json`): what the Isothrum runner
//! executes. It is built from the plan and the typed Compose file this backend renders, never
//! re-parsed from the file's text, by pure functions. Every command is Docker Compose's under
//! the project `${run.name}`, with the Compose file and the runner's pinned-ports file; the
//! message is left to the caller, who fills it from the resolved snapshot.

mod checks;
mod guest;
mod lifecycle;
mod machines;
mod slots;

pub(crate) use guest::guest;

use std::collections::BTreeSet;

use isoloom_manifest::{
    Argv, Checks, ComposeIdentity, Driver, HostArgv, HostOs, Identity, Manifest, ManifestVersion, NonEmpty, PathTemplate, Placeholder, Program, RelPath,
    RunVar, SCHEMA_URL, Template, Tool, Version,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest;

use crate::error::Result;
use crate::file::ComposeFile;
use crate::start::StartPlan;

/// The Compose file, next to the manifest.
const COMPOSE_FILE: &str = "compose.yml";
/// The runner's pinned-ports file, in the run's state folder.
const PORTS_FILE: &str = "ports.yml";

/// The docker target's run manifest for `env`, whose Compose file is `file`; its message is
/// left empty.
pub(crate) fn docker(env: &Environment, file: &ComposeFile) -> Result<Manifest> {
    let members = machines::classify(env, file)?;
    let plan = StartPlan::of(file);
    let runners = checks::runners(env)?;
    let machines = machines::machines(env, file, &members, &Project::Run)?;
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: run_manifest::generator()?,
        lab: run_manifest::lab(env)?,
        target: isoloom_manifest::Target::Docker,
        cloud: None,
        driver: Driver::Compose,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: tools()?,
        identity: identity()?,
        params: slots::params(&machines, &runners.derived)?,
        secrets: slots::slots(env, &members, &plan, slots::Starts::Host)?,
        networks: run_manifest::networks(env, |n| Project::Run.prefixed(n))?,
        outputs: run_manifest::outputs(&machines)?,
        machines,
        lifecycle: lifecycle::lifecycle(env, &plan, &members)?,
        ready: lifecycle::ready(&plan, &members, "up")?,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: checks::setup(file)?,
            runners: runners.runners,
            teardown: Vec::new(),
        },
        recovery: Some(lifecycle::recovery()?),
        expiry: None,
        guest: None,
    })
}

/// `docker compose` under the run's project, with the Compose file and the pinned ports, then
/// `args`.
fn compose(args: &[&str]) -> Result<HostArgv> {
    let mut all = vec![
        Template::text("compose"),
        Template::text("--project-name"),
        Template::from(Placeholder::Run(RunVar::Name)),
        Template::text("--file"),
        Template::text(COMPOSE_FILE),
        Template::text("--file"),
        pinned_ports()?.template().clone(),
    ];
    all.extend(args.iter().map(|a| Template::text(a)));
    Ok(HostArgv::Every(Argv::new(program("docker")?, all)))
}

fn program(name: &str) -> Result<Program> {
    Ok(run_manifest::program(name)?)
}

/// `${run.state_dir}/ports.yml`.
fn pinned_ports() -> Result<PathTemplate> {
    Ok(PathTemplate::in_state_dir(&RelPath::parse(PORTS_FILE)?))
}

/// The Compose project a manifest's commands run under.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Project {
    /// The run's (`${run.name}`): Compose on this computer's Docker, run by the runner.
    Run,
    /// The Compose file's own name: Compose inside a VM, one lab per VM.
    Named(String),
}

impl Project {
    /// `<project>_<name>`: what Compose calls the project's network or volume `name`.
    fn prefixed(&self, name: &str) -> Template {
        match self {
            Project::Run => Template::from_parts([
                isoloom_manifest::Part::Placeholder(Placeholder::Run(RunVar::Name)),
                isoloom_manifest::Part::Text(format!("_{name}")),
            ]),
            Project::Named(project) => Template::text(&format!("{project}_{name}")),
        }
    }
}

/// The project `${run.name}`, the Compose file, then the pinned ports over it.
fn identity() -> Result<Identity> {
    let compose_file = PathTemplate::relative(&RelPath::parse(COMPOSE_FILE)?);
    let pinned = pinned_ports()?;
    Ok(Identity::Compose(ComposeIdentity {
        project: Template::from(Placeholder::Run(RunVar::Name)),
        files: NonEmpty::new(vec![compose_file, pinned.clone()], "the Compose files")?,
        pinned_ports_file: Some(pinned),
    }))
}

/// The first Compose that copies a secret read from the environment into the container with
/// its `uid`, `gid` and `mode`: docker/compose 5c1484e, "apply uid/gid when creating secret from
/// environment" (2022-12-15), first released in v2.14.1. An input or a secret delivered as a
/// file relies on it (a `file:` secret is bind-mounted, and Compose ignores its owner and mode).
const ENVIRONMENT_SECRETS: &str = "2.14.1";

/// The first Compose with `up --wait-timeout`, which the start steps pass: v2.17.0.
const WAIT_TIMEOUT: &str = "2.17.0";

/// The lowest Compose the run needs: the later of [`ENVIRONMENT_SECRETS`] and [`WAIT_TIMEOUT`].
fn compose_min_version() -> Result<Version> {
    Ok(Version::parse(ENVIRONMENT_SECRETS)?.max(Version::parse(WAIT_TIMEOUT)?))
}

/// Docker's engine and Compose v2, at the version the run needs ([`compose_min_version`]).
fn tools() -> Result<Vec<Tool>> {
    let probe = |args: &[&str]| -> Result<HostArgv> { Ok(run_manifest::argv("docker", args)?) };
    let tool = |name: &str, check: HostArgv, min: Option<Version>| -> Result<Tool> {
        let mut t = run_manifest::tool(name, check)?;
        t.min_version = min;
        Ok(t)
    };
    Ok(vec![
        tool("docker", probe(&["info", "--format", "{{.ServerVersion}}"])?, None)?,
        tool("docker-compose", probe(&["compose", "version", "--short"])?, Some(compose_min_version()?))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_is_new_enough_to_copy_environment_secrets_with_their_owner_and_mode() {
        let min = compose_min_version().unwrap();
        assert_eq!(min.as_str(), WAIT_TIMEOUT);
        assert!(min >= Version::parse(ENVIRONMENT_SECRETS).unwrap());
        let compose = tools().unwrap().into_iter().find(|t| t.name.as_str() == "docker-compose").unwrap();
        assert_eq!(compose.min_version, Some(min));
    }
}
