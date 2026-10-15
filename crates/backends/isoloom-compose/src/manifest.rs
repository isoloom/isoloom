//! The `docker` target's run manifest (`.isoloom/docker/run.json`): what the Isothrum runner
//! executes. It is built from the plan and the typed Compose file this backend renders, never
//! re-parsed from the file's text, by pure functions. Every command is Docker Compose's under
//! the project `${run.name}`, with the Compose file and the runner's pinned-ports file; the
//! message is left to the caller, who fills it from the resolved snapshot.

mod checks;
mod lifecycle;
mod machines;
mod slots;

use std::collections::BTreeSet;

use isoloom_manifest::{
    Argv, Checks, Cidr, ComposeIdentity, Driver, Generator, GeneratorName, HostArgv, HostOs, Identity, Lab, LabName, Manifest, ManifestVersion, Name, Network,
    NonEmpty, PathTemplate, Placeholder, Program, RelPath, RunVar, SCHEMA_URL, Template, Tool, ToolName, Version,
};
use isoloom_plan::Environment;

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
    let machines = machines::machines(env, file, &members)?;
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: Generator {
            name: GeneratorName::Isoloom,
            version: Version::parse(env!("CARGO_PKG_VERSION"))?,
        },
        lab: Lab {
            name: LabName::parse(&env.name)?,
            instance: env.instance.map(isoloom_manifest::Instance::new).transpose()?,
            message: None,
        },
        target: isoloom_manifest::Target::Docker,
        cloud: None,
        driver: Driver::Compose,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: tools()?,
        identity: identity()?,
        params: slots::params(&machines, &runners.derived)?,
        secrets: slots::slots(env, &members, &plan)?,
        networks: networks(env)?,
        outputs: machines::outputs(&machines)?,
        machines,
        lifecycle: lifecycle::lifecycle(env, &plan, &members)?,
        ready: lifecycle::ready(&plan, &members)?,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: checks::setup(file)?,
            runners: runners.runners,
            teardown: Vec::new(),
        },
        recovery: Some(lifecycle::recovery()?),
        expiry: None,
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
    Ok(Program::parse(name)?)
}

/// `${run.state_dir}/ports.yml`.
fn pinned_ports() -> Result<PathTemplate> {
    Ok(PathTemplate::in_state_dir(&RelPath::parse(PORTS_FILE)?))
}

/// `${run.name}_<name>`: what Compose calls a project's network or volume.
fn in_project(name: &str) -> Template {
    Template::from_parts([
        isoloom_manifest::Part::Placeholder(Placeholder::Run(RunVar::Name)),
        isoloom_manifest::Part::Text(format!("_{name}")),
    ])
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

/// Docker's engine and Compose v2 (`--wait-timeout` needs 2.17).
fn tools() -> Result<Vec<Tool>> {
    let probe = |args: &[&str]| -> Result<HostArgv> { Ok(HostArgv::Every(Argv::new(program("docker")?, args.iter().map(|a| Template::text(a)).collect()))) };
    let tool = |name: &str, check: HostArgv, min: Option<&str>| -> Result<Tool> {
        Ok(Tool {
            name: ToolName::parse(name)?,
            probe: check,
            min_version: min.map(Version::parse).transpose()?,
            container: None,
            container_hosts: BTreeSet::new(),
            hosts: BTreeSet::new(),
            providers: BTreeSet::new(),
        })
    };
    Ok(vec![
        tool("docker", probe(&["info", "--format", "{{.ServerVersion}}"])?, None)?,
        tool("docker-compose", probe(&["compose", "version", "--short"])?, Some("2.17.0"))?,
    ])
}

/// The lab's networks, on their Docker blocks, under the project's name.
fn networks(env: &Environment) -> Result<Vec<Network>> {
    env.networks
        .iter()
        .map(|(name, n)| {
            Ok(Network {
                name: Name::parse(name)?,
                cidr: Cidr::parse(&n.cidr)?,
                internet: n.internet,
                native: Some(in_project(name)),
            })
        })
        .collect()
}
