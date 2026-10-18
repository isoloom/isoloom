//! The Compose file of a planned [`Environment`]: a service per container machine (with its
//! `init:` jobs and network sidecar), Isoloom's router, VLAN switches, probes, check runners and
//! tools, then the networks and volumes; every resource labelled as the environment's.

mod configs;
mod hardware;
mod process;
mod secrets;
mod service;

pub(crate) use service::readiness;

use std::fmt;

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::backend::Files;
use isoloom_render::container::PROBE_IMAGE;
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, trunks};
use isoloom_spec::model::{Arch, Target};
use isoloom_spec::validate::Cidr;

use crate::error::{Error, Result};
use crate::file::{Attachment, ComposeFile, NetworkMode, Restart, Service, Volume};
use crate::images::TOOLBOX_IMAGE;
use crate::layout::DIR;
use crate::machine::Machine;
use crate::names::{probe_service, tool_service};
use crate::{check_runners, ipv6, labels, networks, outside, router, storage};

/// How to start, check and stop the file by hand, at its top.
const USAGE: &str = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: isothrum check . (or: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check, and isoloom-check-<machine>)\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";

/// The Compose file of `env` and the files it mounts (check runners, appliance configurations),
/// refused as `target` when `env` needs what Compose can't do.
///
/// # Errors
///
/// [`Error::Refused`] when `env` needs what Compose can't do, [`Error::Unwritable`] when the
/// file can't be written as YAML.
pub fn render(target: Target, env: &Environment) -> Result<Files> {
    let Rendered { file, files: mounted } = compose_file(target, env)?;
    let yaml = to_yaml(target, &file)?;
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{USAGE}{}{}{}\n{yaml}", header("#"), secrets::usage(&file), configs::usage(env), Moved(env)),
    }];
    files.extend(mounted);
    Ok(files)
}

/// A Compose file before it is written, and the files it mounts.
#[derive(Debug, Clone)]
pub(crate) struct Rendered {
    /// The Compose file.
    pub(crate) file: ComposeFile,
    /// The files next to it: check runners, appliance configurations, Dockerfiles.
    pub(crate) files: Files,
}

/// The typed Compose file of `env` and the files it mounts, for a target that adapts it before
/// writing it (`hybrid` plugs it into its Docker host's networks). Refused as `target` when
/// `env` needs what Compose can't do.
pub(crate) fn compose_file(target: Target, env: &Environment) -> Result<Rendered> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &crate::backend::capabilities()) {
        return Err(Error::Refused { target, refusal });
    }
    let mut out = Out::default();
    let ts = trunks::containers(env.spec());
    router::services(env, &ts, &mut out.services);
    for m in Machine::containers(env) {
        service::machine(env, m, &ts, &mut out);
    }
    // The access box (the learner's attacker box) renders as an on-demand service even when it is
    // `Supplied` (no `docker:`/`vm:` form of its own): the runner hot-adds it. Container-shape
    // access boxes are already covered by `containers()` above.
    for m in Machine::supplied_access(env) {
        service::machine(env, m, &ts, &mut out);
    }
    storage::folders(env, &mut out.services);
    let runners = check_runners::services(env, &mut out.services);
    tools(env, &mut out.services);
    let mut file = ComposeFile {
        name: env.name.clone(),
        networks: networks::declared(env, &ts),
        volumes: volumes(env, &out.probe_archs),
        secrets: secrets::of_file(env),
        configs: configs::declared(env),
        ..ComposeFile::default()
    };
    probes(env, &out.probe_archs, &mut out.services);
    file.services = out.services;
    labels::mark(env, &mut file);
    // Existing networks aren't the environment's: unlabelled, so nothing collects them.
    file.networks.extend(outside::networks(env));
    let mut files = runners;
    files.extend(out.files);
    files.extend(isoloom_render::configure::dockerfile::files(env));
    Ok(Rendered { file, files })
}

/// `file` as YAML, a failure reported as `target`'s.
pub(crate) fn to_yaml(target: Target, file: &ComposeFile) -> Result<String> {
    serde_yaml_ng::to_string(file).map_err(|source| Error::Unwritable { target, source })
}

/// What rendering the machines collects besides their services.
#[derive(Default)]
struct Out {
    services: IndexMap<String, Service>,
    /// Appliance configurations and Dockerfiles, next to the Compose file.
    files: Vec<GeneratedFile>,
    /// The architectures of machines that need the probe: one copy job (and volume) each.
    probe_archs: Vec<Arch>,
}

/// The comment saying which networks moved to their Docker block, a line each.
struct Moved<'a>(&'a Environment);

impl fmt::Display for Moved<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, n) in self.0.networks.iter().filter(|(_, n)| n.declared_cidr != n.cidr) {
            writeln!(
                f,
                "# On Docker, network `{name}` uses {} instead of {} (same last octets).",
                n.cidr, n.declared_cidr
            )?;
        }
        Ok(())
    }
}

/// Tools: observers on every network at the reserved addresses, outside the environment's
/// contract (no reach rule, no check names them).
fn tools(env: &Environment, services: &mut IndexMap<String, Service>) {
    let spec = env.spec();
    for (i, (name, tool)) in spec.tools.iter().enumerate() {
        let shell = name == "shell";
        let mut t = Service {
            image: Some(env.images.pull(tool.image.as_deref().unwrap_or(TOOLBOX_IMAGE))),
            hostname: Some(name.clone()),
            command: tool.command.clone(),
            restart: Some(Restart::UnlessStopped),
            ..Service::default()
        };
        if tool.command.is_empty() && shell {
            t.init = true;
            t.entrypoint = vec!["sleep".into(), "infinity".into()];
        }
        if shell {
            t.cap_add = vec!["NET_ADMIN".into(), "NET_RAW".into()];
        }
        // The spec is validated: every block parses.
        for (net, n) in &spec.networks {
            // On an IPv6-only network, Docker hands the tool its address.
            if ipv6::only(env, net) {
                t.networks.insert(net.clone(), None);
            } else if let Some(cidr) = Cidr::parse(&n.cidr) {
                t.networks.insert(net.clone(), Some(Attachment::at(cidr.tool(i))));
            }
        }
        if let (Some(port), Some(host)) = (tool.port, tool.publish) {
            t.ports = vec![format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{host}:{port}")];
        }
        services.insert(tool_service(name), t);
    }
}

/// The one-shot services that copy the probe, one per architecture.
fn probes(env: &Environment, archs: &[Arch], services: &mut IndexMap<String, Service>) {
    for arch in archs {
        let name = probe_service(*arch);
        let p = Service {
            image: Some(env.images.pull(PROBE_IMAGE)),
            platform: Some(arch.docker_platform().to_owned()),
            entrypoint: vec!["/bin/cp".into(), "/bin/busybox".into(), "/probe/busybox".into()],
            volumes: vec![format!("{name}:/probe").into()],
            network_mode: Some(NetworkMode::None),
            restart: Some(Restart::No),
            ..Service::default()
        };
        services.insert(name, p);
    }
}

/// The named volumes: the machines' (and their disks'), then the probes'.
fn volumes(env: &Environment, archs: &[Arch]) -> IndexMap<String, Volume> {
    let mut out = storage::declared(env);
    out.extend(archs.iter().map(|a| (probe_service(*a), Volume::default())));
    out
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_spec::model::Target;

    use crate::start::{StartPlan, start_plan};

    #[test]
    fn the_start_plan_reads_the_same_from_the_text_as_from_the_typed_file() {
        for example in crate::testing::examples() {
            let env = Environment::build(&example.spec, Target::Docker, None);
            let Ok(rendered) = super::compose_file(Target::Docker, &env) else { continue };
            let yaml = super::to_yaml(Target::Docker, &rendered.file).expect("writes");
            assert_eq!(StartPlan::of(&rendered.file), start_plan(&yaml).expect("reads back"), "{}", example.name);
        }
    }
}
