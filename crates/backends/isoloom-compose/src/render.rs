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
use isoloom_manifest::contract::{OUTPUT_DIR, PUBLISH_ADDRESS};
use isoloom_plan::Environment;
use isoloom_plan::checks::Position;
use isoloom_render::backend::Files;
use isoloom_render::container::PROBE_IMAGE;
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::model::Arch;

use crate::backend::ComposeTarget;
use crate::error::{Error, Result};
use crate::file::{Attachment, ComposeFile, NetworkMode, RO_BIND, Restart, Service, Volume};
use crate::trunks;
use isoloom_render::run_manifest::RUNNER;

use crate::layout::DIR;
use crate::machine::Machine;
use crate::names::{check_runner_service, probe_service, tool_service};
use crate::start::StartPlan;
use crate::{check_runners, ipv6, labels, networks, outside, router, storage};

/// How to start, check and stop `file` by hand, at its top: started as its [`StartPlan`] says,
/// so a set-up job nothing waits for runs after the rest is up.
fn usage(file: &ComposeFile) -> String {
    let compose = format!("docker compose -f {OUTPUT_DIR}/{DIR}/compose.yml");
    let start = StartPlan::of(file).command(&compose, None);
    // The runner where a user stands, and another position's.
    let user = Position::Networks;
    let (own, other) = (
        check_runner_service(&user, &user),
        check_runner_service(&Position::Machine("<machine>".into()), &user),
    );
    format!("# Start:  {start}\n# Checks: {RUNNER}, or by hand: {compose} --profile check run --rm {own}, and {other}\n# Stop:   {compose} down -v\n")
}

/// The Compose file of `env` and the files it mounts (check runners, appliance configurations),
/// refused as `target` when `env` needs what Compose can't do.
///
/// # Errors
///
/// [`Error::Refused`] when `env` needs what Compose can't do, [`Error::Unwritable`] when the
/// file can't be written as YAML.
pub(crate) fn render(target: ComposeTarget, env: &Environment) -> Result<Files> {
    let Rendered { file, files: mounted } = compose_file(target, env)?;
    let yaml = to_yaml(target, &file)?;
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!(
            "{}{}{}{}{}\n{yaml}",
            header("#"),
            usage(&file),
            secrets::usage(env, &file),
            configs::usage(env),
            Moved(env)
        ),
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
pub(crate) fn compose_file(target: ComposeTarget, env: &Environment) -> Result<Rendered> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &crate::backend::capabilities(target)) {
        return Err(Error::Refused {
            target: target.target(),
            refusal,
        });
    }
    let mut out = Out::default();
    let ts = trunks::containers(env.spec());
    router::services(env, &ts, &mut out.services);
    for m in Machine::containers(env) {
        service::machine(env, m, &ts, &mut out);
    }
    // The access machine renders as an on-demand service even when it is
    // `Supplied` (no `docker:`/`vm:` form of its own): the runner hot-adds it. Container-shape
    // access machines are already covered by `containers()` above.
    for m in Machine::supplied_access(env) {
        service::machine(env, m, &ts, &mut out);
    }
    storage::folders(env, &mut out.services);
    let runners = check_runners::services(env, &mut out.services);
    tools(env, &mut out.services);
    let mut file = ComposeFile {
        name: env.name.clone(),
        networks: networks::declared(env, &ts),
        volumes: volumes(env, &out.probes),
        secrets: secrets::of_file(env),
        configs: configs::declared(env),
        ..ComposeFile::default()
    };
    probes(env, &out.probes, &mut out.services);
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
pub(crate) fn to_yaml(target: ComposeTarget, file: &ComposeFile) -> Result<String> {
    serde_yaml_ng::to_string(file).map_err(|source| Error::Unwritable {
        target: target.target(),
        source,
    })
}

/// What rendering the machines collects besides their services.
#[derive(Default)]
struct Out {
    services: IndexMap<String, Service>,
    /// Appliance configurations and Dockerfiles, next to the Compose file.
    files: Vec<GeneratedFile>,
    /// The machines that need the probe, by the architecture they are pinned to (`None`: the
    /// host's): one copy job (and volume) each.
    probes: Probes,
}

/// The machines that need the probe, by the architecture they are pinned to (`None`: the
/// host's), in the order first met.
type Probes = Vec<(Option<Arch>, Vec<String>)>;

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
    for (name, plan) in &env.tools {
        let tool = &plan.tool;
        let shell = name == "shell";
        let mut t = Service {
            image: Some(env.images.pull(&plan.image)),
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
        for (net, ip) in &plan.addresses {
            // On an IPv6-only network, Docker hands the tool its address.
            let at = (!ipv6::only(env, net)).then(|| Attachment::at(*ip));
            t.networks.insert(net.clone(), at);
        }
        if let (Some(port), Some(host)) = (tool.port, tool.publish) {
            t.ports = vec![format!("${{{PUBLISH_ADDRESS}:-127.0.0.1}}:{host}:{port}")];
        }
        services.insert(tool_service(name), t);
    }
}

/// Where a pinned architecture's probe job finds the host's busybox.
const HOST_PROBE: &str = "/isoloom-host-probe";

/// The one-shot services that copy the probe into a volume each: the host's own busybox for the
/// machines that run on the host's architecture, and one built for each architecture machines
/// are pinned to. A pinned one runs the host's busybox first (an architecture the host can't run
/// can't even say so), to say exactly what is missing rather than the engine's "exec format
/// error".
fn probes(env: &Environment, probes: &Probes, services: &mut IndexMap<String, Service>) {
    if probes.is_empty() {
        return;
    }
    let host = probe_service(None);
    let job = |name: &str| Service {
        image: Some(env.images.pull(PROBE_IMAGE)),
        entrypoint: vec!["/bin/cp".into(), "/bin/busybox".into(), "/probe/busybox".into()],
        volumes: vec![format!("{name}:/probe").into()],
        network_mode: Some(NetworkMode::None),
        restart: Some(Restart::No),
        ..Service::default()
    };
    services.insert(host.clone(), job(&host));
    for (arch, machines) in probes.iter().filter_map(|(a, m)| Some(((*a)?, m))) {
        let name = probe_service(Some(arch));
        let mut p = job(&name);
        p.platform = Some(arch.docker_platform().to_owned());
        p.entrypoint = vec![
            format!("{HOST_PROBE}/busybox"),
            "sh".into(),
            "-c".into(),
            crate::interpolation::literal(&unrunnable(arch, machines)),
        ];
        p.volumes.push(format!("{host}:{HOST_PROBE}:{RO_BIND}").into());
        p.after(host.clone(), crate::file::Condition::CompletedSuccessfully);
        services.insert(name, p);
    }
}

/// The pinned probe's script: its own busybox copied when the host runs `arch`, else what to do
/// about it, naming the `machines` pinned to it.
fn unrunnable(arch: Arch, machines: &[String]) -> String {
    let platform = arch.docker_platform();
    let id = arch.id();
    let who = match machines {
        [one] => format!("machine {one} needs"),
        _ => format!("machines {} need", machines.join(", ")),
    };
    format!(
        "if /bin/busybox true 2>/dev/null; then exec /bin/busybox cp /bin/busybox /probe/busybox; fi\n\
         echo \"isoloom: this Docker host ($({HOST_PROBE}/busybox uname -m)) can't run {platform} programs, which {who} (pinned by the spec's hardware.arch, or an image published for {id} only). \
         Turn on {id} emulation: Docker Desktop has it; on Linux, register QEMU's with binfmt (docker run --privileged --rm tonistiigi/binfmt --install {id}, or the qemu-user-static package). \
         Or run the environment on a {platform} host.\" >&2\n\
         exit 1\n"
    )
}

/// The named volumes: the machines' (and their disks'), then the probes' (the host's beside a
/// pinned one: its job runs the host's busybox).
fn volumes(env: &Environment, probes: &Probes) -> IndexMap<String, Volume> {
    let mut out = storage::declared(env);
    if !probes.is_empty() {
        out.insert(probe_service(None), Volume::default());
    }
    out.extend(probes.iter().filter(|(a, _)| a.is_some()).map(|(a, _)| (probe_service(*a), Volume::default())));
    out
}
