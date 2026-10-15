//! A machine's Compose service: its image or build, runtime settings, networks, appliance files,
//! readiness check, then its sidecar and `init:` jobs.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Image, PublishedPort};
use isoloom_render::container::{PROBE_DIR, init_names};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, constraints, trunks};
use isoloom_spec::model;

use super::{Out, configs, hardware, process, secrets};
use crate::appliances::{self, Port};
use crate::file::{Attachment, Build, Condition, Deploy, Limits, NetworkMode, Resources, Restart, Service, Ulimit};
use crate::interpolation::literal;
use crate::layout::{DIR, ROOT, file_name, relative_to};
use crate::machine::Machine;
use crate::names::{probe_service, routes_service};
use crate::{labels, networks, outside, sidecars, storage};

/// A machine's service, then its sidecar and its `init:` jobs.
pub(super) fn machine(env: &Environment, m: Machine<'_>, ts: &[trunks::Trunk], out: &mut Out) {
    let mut svc = base(env, m, ts);
    appliance(env, m, &mut svc, &mut out.files);
    waits(env, m, &mut svc, &mut out.probe_archs);
    svc.deploy = limits(m.plan);
    svc.labels.extend(labels::ports(m.spec));
    hardware::machine(&m.plan.hardware, &mut svc);
    out.services.insert(m.name.to_owned(), svc);
    if sidecars::needed(env, m) {
        out.services.insert(routes_service(m.name), sidecars::routes(env, m.name, m));
    }
    init_jobs(env, m, &mut out.services);
}

/// What a machine that waits for `m` waits for: healthy (its probe passes) when it has a port
/// to probe or a healthcheck of its own, else started.
pub(crate) fn readiness(m: &model::Machine) -> Condition {
    if m.has_readiness() { Condition::Healthy } else { Condition::Started }
}

/// What every machine's service has: its image, runtime settings, names and networks.
fn base(env: &Environment, m: Machine<'_>, ts: &[trunks::Trunk]) -> Service {
    let (spec, plan) = (m.spec, m.plan);
    let (tmpfs, long_tmpfs) = storage::tmpfs(spec);
    let mut svc = Service {
        platform: Some(plan.arch.docker_platform().to_owned()),
        privileged: spec.privileged,
        read_only: spec.read_only,
        sysctls: plan.settings.sysctls.clone(),
        ulimits: plan
            .settings
            .ulimits
            .iter()
            .map(|(k, u)| (k.clone(), Ulimit::new(u.soft(), u.hard())))
            .collect(),
        shm_size: spec.shm_size.clone(),
        tmpfs,
        hostname: Some(m.name.to_owned()),
        networks: networks::attachments(env, plan),
        extra_hosts: networks::extra_hosts(env, m),
        environment: environment(env, plan),
        ports: plan.ports.iter().map(port).collect(),
        volumes: storage::volume_mounts(plan).into_iter().chain(long_tmpfs).collect(),
        secrets: secrets::of_machine(plan),
        configs: configs::mounts(m.name, plan),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    };
    if let Some(Image::Container { reference, build }) = &plan.image {
        svc.image = Some(reference.clone());
        svc.build = build.as_ref().map(|b| build_of(env, b));
    }
    if let Some(dns) = &spec.dns {
        svc.dns.clone_from(&dns.servers);
        svc.dns_opt.clone_from(&dns.options);
        svc.dns_search.clone_from(&dns.search);
        svc.domainname.clone_from(&dns.domain);
    }
    // A stock image the runner supplies for the user to work from, or one the spec marks
    // `idle`: kept running idle (its own command may be a shell that exits at once).
    if spec.supplied || spec.docker.as_ref().is_some_and(|d| d.idle) {
        svc.entrypoint = vec!["sleep".into(), "infinity".into()];
    }
    process::apply(plan, &mut svc);
    // A VLAN it reaches through a trunk isn't an interface of its own: the trunk is.
    svc.networks.retain(|k, _| !trunks::carried(ts, m.name, k));
    for t in trunks::of(ts, m.name) {
        svc.networks
            .insert(trunks::link_network(t), Some(Attachment::with_mac(trunks::mac(t, trunks::End::Machine))));
    }
    svc.networks.extend(outside::attachments(plan));
    if plan.is_gateway {
        // It routes: forwarding on, and the right to set its own firewall rules.
        svc.cap_add = vec!["NET_ADMIN".into()];
        svc.sysctls = IndexMap::from([("net.ipv4.ip_forward".to_owned(), "1".to_owned())]);
    }
    svc
}

/// A published port: on the host's loopback, on an ephemeral host port by default (no
/// collisions); `ISOLOOM_PUBLISH_FIXED` pins it to `publish` inside the docker-vm/cloud VMs that
/// forward it. `<port>/udp` for a UDP service (Compose's default is TCP).
fn port(p: &PublishedPort) -> String {
    let proto = if p.protocol.is_tcp() { "" } else { "/udp" };
    let (publish, port) = (p.publish, p.port);
    if p.fixed {
        format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{publish}:{port}{proto}")
    } else {
        format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:${{ISOLOOM_PUBLISH_FIXED:+{publish}}}:{port}{proto}")
    }
}

/// A machine's Compose `build`: its context, and the Dockerfile (Compose reads it relative to
/// the context) and build arguments when the spec gives them; the spec's own arguments win over
/// the constraints'. `$$`: a literal value, not Compose's interpolation. A generated Dockerfile
/// builds with the project folder as its context.
fn build_of(env: &Environment, b: &isoloom_plan::Build) -> Build {
    let constraints = constraints::build_args(env.spec());
    let (context, dockerfile, target, own) = match b {
        isoloom_plan::Build::Own {
            context,
            dockerfile,
            target,
            args,
        } => (
            format!("{ROOT}/{context}"),
            dockerfile.as_ref().map(|f| relative_to(context, f)),
            target.clone(),
            args.clone(),
        ),
        isoloom_plan::Build::Generated { dockerfile, .. } => (ROOT.to_owned(), Some(dockerfile.clone()), None, IndexMap::new()),
    };
    Build {
        context,
        dockerfile,
        target,
        args: constraints
            .into_iter()
            .filter(|(k, _)| !own.contains_key(k))
            .chain(own.clone())
            .map(|(k, v)| (k, literal(&v)))
            .collect(),
        labels: IndexMap::new(),
    }
}

/// A machine's environment: the spec's constraints (proxy, package mirrors), its own fixed
/// values (`$$`: literal, not Compose's interpolation), then its inputs, read from the shell.
fn environment(env: &Environment, plan: &isoloom_plan::MachinePlan) -> Option<IndexMap<String, String>> {
    let constraints = constraints::machine_env(env.spec());
    if plan.inputs.is_empty() && constraints.is_empty() && plan.settings.environment.is_empty() {
        return None;
    }
    let fixed = plan.settings.environment.iter().map(|(k, v)| (k.clone(), v.clone()));
    let mut out: IndexMap<String, String> = constraints.into_iter().chain(fixed).map(|(k, v)| (k, literal(&v))).collect();
    out.extend(plan.inputs.iter().map(|i| (i.clone(), format!("${{{i}:-}}"))));
    Some(out)
}

/// A network appliance's files, start and interfaces in the order it expects (nothing for a
/// machine that isn't one).
fn appliance(env: &Environment, m: Machine<'_>, svc: &mut Service, files: &mut Vec<GeneratedFile>) {
    let Some((d, kind)) = m.spec.docker.as_ref().and_then(|d| Some((d, d.appliance?))) else {
        return;
    };
    let spec = env.spec();
    let name = m.name;
    let w = appliances::wiring(spec, name, m.spec, kind, |n, o| isoloom_plan::addressing::address(spec, n, o));
    svc.volumes.clear();
    for f in w.files {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/{}", f.name),
            contents: f.contents,
        });
        svc.volumes.push(format!("./appliances/{name}/{}:{}:ro", f.name, f.inside).into());
    }
    if let Some(own) = &d.config {
        svc.volumes.push(format!("{ROOT}/{own}:{}:ro", w.own_config).into());
    }
    if let (Some(fw), Some(inside)) = (&d.firmware, w.firmware) {
        svc.volumes.push(format!("{ROOT}/{fw}:{inside}:ro").into());
    }
    // An emulator Isoloom builds itself (its Dockerfile next to the Compose file).
    if let Some(dockerfile) = w.build {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/build/Dockerfile"),
            contents: pull_from(env, dockerfile),
        });
        svc.image = None;
        svc.build = Some(Build {
            context: format!("./appliances/{name}/build"),
            ..Build::default()
        });
    }
    // `$$`: the script's own variables, not Compose's interpolation.
    svc.entrypoint = w.entrypoint.iter().map(|e| literal(e)).collect();
    svc.environment = Some(w.environment.into_iter().collect());
    if w.privileged {
        svc.privileged = true;
    } else {
        svc.cap_add = vec!["NET_ADMIN".into(), "NET_RAW".into()];
    }
    svc.networks = appliance_networks(spec, name, &w.ports);
}

/// The interface names the image expects, set outright (Compose 2.36+, Docker 28.1+): the order
/// Docker attaches networks in isn't theirs to rely on.
fn appliance_networks(spec: &isoloom_spec::model::Spec, name: &str, ports: &[Port]) -> IndexMap<String, Option<Attachment>> {
    let mgmt = Attachment {
        interface_name: Some("eth0".into()),
        priority: Some(1000),
        ..Attachment::at(appliances::mgmt_address(spec, name))
    };
    let mut nets = IndexMap::from([(appliances::MGMT_NETWORK.to_owned(), Some(mgmt))]);
    for (k, p) in ports.iter().enumerate() {
        let a = Attachment {
            ipv4_address: p.address.map(|a| a.to_string()),
            interface_name: Some(format!("eth{}", k + 1)),
            priority: Some(999u32.saturating_sub(u32::try_from(k).unwrap_or(u32::MAX))),
            ..Attachment::default()
        };
        nets.insert(p.network.clone(), Some(a));
    }
    nets
}

/// A Dockerfile's `FROM` lines through the registry mirrors.
fn pull_from(env: &Environment, dockerfile: &str) -> String {
    dockerfile
        .split_inclusive('\n')
        .map(|line| match line.strip_prefix("FROM ") {
            Some(rest) => {
                let (image, tail) = rest.find(char::is_whitespace).and_then(|at| rest.split_at_checked(at)).unwrap_or((rest, ""));
                format!("FROM {}{tail}", env.images.pull(image))
            }
            None => line.to_owned(),
        })
        .collect()
}

/// The healthcheck (the port probe and the machine's own) and what the machine waits for before
/// it starts.
fn waits(env: &Environment, m: Machine<'_>, svc: &mut Service, archs: &mut Vec<model::Arch>) {
    let plan = m.plan;
    if let Some(healthcheck) = process::healthcheck(m.spec, plan) {
        svc.healthcheck = Some(healthcheck);
        // Isoloom's own probe, copied into a volume before the machine starts.
        let probe_svc = probe_service(plan.arch);
        svc.volumes.push(format!("{probe_svc}:{PROBE_DIR}:ro").into());
        svc.after(probe_svc, Condition::CompletedSuccessfully);
        if !archs.contains(&plan.arch) {
            archs.push(plan.arch);
        }
    }
    for dep in plan.depends_on.iter().filter_map(|d| Machine::of(env, d)) {
        svc.after(dep.name, readiness(dep.spec));
        for init in init_names(dep.name, dep.spec) {
            svc.after(init, Condition::CompletedSuccessfully);
        }
        if sidecars::needed(env, dep) {
            svc.after(routes_service(dep.name), Condition::Healthy);
        }
    }
}

/// The machine's CPU and memory limits, when it has some.
fn limits(plan: &isoloom_plan::MachinePlan) -> Option<Deploy> {
    let limits = Limits {
        cpus: plan.size.cpus.map(|c| c.to_string()),
        memory: plan.size.memory_mb.map(|mb| format!("{mb}M")),
    };
    (limits.cpus.is_some() || limits.memory.is_some()).then_some(Deploy {
        resources: Resources {
            limits: Some(limits),
            reservations: None,
        },
    })
}

/// One-shot init jobs, in order, each after the previous one. They share the machine's network
/// namespace (same addresses, names and routes) and its volumes, as a VM's steps see its disk.
fn init_jobs(env: &Environment, m: Machine<'_>, services: &mut IndexMap<String, Service>) {
    let Some(d) = &m.spec.docker else { return };
    let plan = m.plan;
    let image = match &plan.image {
        Some(Image::Container { reference, .. }) => Some(reference.clone()),
        _ => None,
    };
    let mut previous: Option<String> = None;
    for (job, script) in init_names(m.name, m.spec).into_iter().zip(&d.init) {
        let mounted = format!("/isoloom/init/{}", file_name(script));
        // A setup job runs as root with what the machine has: its network, volumes, files (value
        // files included) and privileges.
        let mut j = Service {
            image: image.clone(),
            user: Some("root".into()),
            privileged: m.spec.privileged,
            cap_add: if plan.is_gateway { vec!["NET_ADMIN".into()] } else { Vec::new() },
            entrypoint: vec!["/bin/sh".into(), mounted.clone()],
            network_mode: Some(NetworkMode::Service(m.name.to_owned())),
            environment: environment(env, plan),
            secrets: secrets::of_machine(plan),
            configs: configs::mounts(m.name, plan),
            restart: Some(Restart::No),
            ..Service::default()
        };
        j.volumes.push(format!("{ROOT}/{script}:{mounted}:ro").into());
        j.volumes.extend(storage::volume_mounts(plan));
        j.after(m.name, readiness(m.spec));
        if sidecars::needed(env, m) {
            j.after(routes_service(m.name), Condition::Healthy);
        }
        if let Some(p) = &previous {
            j.after(p.clone(), Condition::CompletedSuccessfully);
        }
        services.insert(job.clone(), j);
        previous = Some(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_from_line_keeps_its_stage_name_and_other_lines_stay() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
        );
        let dockerfile = "FROM ubuntu:24.04 AS base\nRUN true\nFROM ubuntu:24.04\n";
        assert_eq!(pull_from(&env, dockerfile), dockerfile);
    }

    #[test]
    fn appliance_interfaces_attach_in_order_from_the_highest_priority() {
        let spec = isoloom_plan::Lab::parse(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/24 } }\nmachines:\n  r: { networks: { a: 1, b: 1 }, app: { image: iol, appliance: cisco-iol } }\n",
        )
        .expect("parses")
        .for_target(isoloom_spec::model::Target::Docker);
        let ports = appliances::ports(&spec, "r", &spec.machines["r"], &|n, o| isoloom_plan::addressing::address(&spec, n, o));
        let nets = appliance_networks(&spec, "r", &ports);
        let order: Vec<(&str, Option<u32>)> = nets.iter().map(|(n, a)| (n.as_str(), a.as_ref().and_then(|a| a.priority))).collect();
        assert_eq!(order, [("isoloom-mgmt", Some(1000)), ("a", Some(999)), ("b", Some(998))]);
    }
}
