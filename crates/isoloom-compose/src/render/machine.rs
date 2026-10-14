//! A machine's Compose service: its image or build, runtime settings, networks, appliance files,
//! readiness check, then its sidecar and `init:` jobs.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Image, MachinePlan, PublishedPort};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, constraints, trunks};
use isoloom_spec::model::{Arch, Machine};

use super::Out;
use crate::appliances;
use crate::docker::{DIR, PROBE_DIR, ROOT, file_name, init_names, probe, probe_service, ready_condition, relative_to};
use crate::file::{Attachment, Build, Deploy, Healthcheck, Limits, Resources, Service, Ulimit};
use crate::sidecars;

/// A machine's service, then its sidecar and its `init:` jobs.
pub(super) fn machine(env: &Environment, name: &str, mp: &MachinePlan, ts: &[trunks::Trunk], out: &mut Out) {
    let m = &env.spec().machines[name];
    let mut svc = base(env, name, mp, m, ts);
    appliance(env, name, m, &mut svc, &mut out.files);
    readiness(env, mp, m, &mut svc, &mut out.probe_archs);
    out.services.insert(name.to_string(), svc);
    if sidecars::needs_routes(env, name) {
        out.services.insert(format!("{name}-routes"), sidecars::routes(env, name, name));
    }
    init_jobs(env, name, mp, m, &mut out.services);
}

/// What every machine's service has: its image, runtime settings, names and networks.
fn base(env: &Environment, name: &str, mp: &MachinePlan, m: &Machine, ts: &[trunks::Trunk]) -> Service {
    let d = m.docker.as_ref();
    let mut svc = Service {
        platform: Some(mp.arch.docker_platform().to_string()),
        privileged: m.privileged,
        read_only: m.read_only,
        sysctls: mp.settings.sysctls.clone(),
        ulimits: mp.settings.ulimits.iter().map(|(k, u)| (k.clone(), ulimit(u.soft(), u.hard()))).collect(),
        shm_size: m.shm_size.clone(),
        tmpfs: m.tmpfs.clone(),
        hostname: Some(name.to_string()),
        networks: sidecars::attachments(env, name, mp),
        extra_hosts: sidecars::extra_hosts(env, name),
        environment: environment(env, mp),
        ports: mp.ports.iter().copied().map(port).collect(),
        volumes: mp.volumes.iter().map(|v| format!("{}:{}", v.key, v.path)).collect(),
        restart: Some("unless-stopped".into()),
        ..Service::default()
    };
    if let Some(Image::Container { reference, build }) = &mp.image {
        svc.image = Some(reference.clone());
        svc.build = build.as_ref().map(|b| build_of(env, b));
    }
    if let Some(dns) = &m.dns {
        svc.dns.clone_from(&dns.servers);
        svc.dns_search.clone_from(&dns.search);
        svc.domainname.clone_from(&dns.domain);
    }
    // A stock image the runner supplies for the user to work from, or one the spec marks
    // `idle`: kept running idle (its own command may be a shell that exits at once).
    if m.supplied || d.is_some_and(|d| d.idle) {
        svc.entrypoint = vec!["sleep".into(), "infinity".into()];
    }
    // A VLAN it reaches through a trunk isn't an interface of its own: the trunk is.
    svc.networks.retain(|k, _| !trunks::carried(ts, name, k));
    for t in trunks::of(ts, name) {
        svc.networks.insert(trunks::link_network(t), mac(&trunks::mac(t, 0)));
    }
    if mp.is_gateway {
        // It routes: forwarding on, and the right to set its own firewall rules.
        svc.cap_add = vec!["NET_ADMIN".into()];
        svc.sysctls = IndexMap::from([("net.ipv4.ip_forward".to_string(), "1".to_string())]);
    }
    svc
}

/// A network attachment with a fixed MAC address (a trunk's end, found by it).
#[allow(clippy::unnecessary_wraps)] // the value of a service's `networks` map, as `Attachment::at`.
pub(crate) fn mac(mac: &str) -> Option<Attachment> {
    Some(Attachment {
        mac_address: Some(mac.to_string()),
        ..Attachment::default()
    })
}

fn ulimit(soft: u64, hard: u64) -> Ulimit {
    if soft == hard { Ulimit::Both(soft) } else { Ulimit::Range { soft, hard } }
}

/// A published port: on the host's loopback, on an ephemeral host port by default (no
/// collisions); ISOLOOM_PUBLISH_FIXED pins it to `publish` inside the docker-vm/cloud VMs that
/// forward it. `<port>/udp` for a UDP service (Compose's default is TCP).
pub(super) fn port(p: PublishedPort) -> String {
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
/// the constraints'. `$$`: a literal value, not Compose's interpolation.
fn build_of(env: &Environment, b: &isoloom_plan::environment::Build) -> Build {
    let constraints = constraints::build_args(env.spec());
    let own = b.args.iter().map(|(k, v)| (k.clone(), v.clone()));
    Build {
        context: format!("{ROOT}/{}", b.context),
        dockerfile: b.dockerfile.as_ref().map(|f| relative_to(&b.context, f)),
        args: constraints
            .into_iter()
            .filter(|(k, _)| !b.args.contains_key(k))
            .chain(own)
            .map(|(k, v)| (k, v.replace('$', "$$")))
            .collect(),
        labels: IndexMap::new(),
    }
}

/// A machine's environment: the spec's constraints (proxy, package mirrors), its own fixed
/// values (`$$`: literal, not Compose's interpolation), then its inputs, read from the shell.
pub(crate) fn environment(env: &Environment, mp: &MachinePlan) -> Option<IndexMap<String, String>> {
    let constraints = constraints::machine_env(env.spec());
    if mp.inputs.is_empty() && constraints.is_empty() && mp.settings.environment.is_empty() {
        return None;
    }
    let fixed = mp.settings.environment.iter().map(|(k, v)| (k.clone(), v.clone()));
    let mut out: IndexMap<String, String> = constraints.into_iter().chain(fixed).map(|(k, v)| (k, v.replace('$', "$$"))).collect();
    out.extend(mp.inputs.iter().map(|i| (i.clone(), format!("${{{i}:-}}"))));
    Some(out)
}

/// A network appliance's files, start and interfaces in the order it expects (nothing for a
/// machine that isn't one).
fn appliance(env: &Environment, name: &str, m: &Machine, svc: &mut Service, files: &mut Vec<GeneratedFile>) {
    let Some((d, kind)) = m.docker.as_ref().and_then(|d| Some((d, d.appliance?))) else {
        return;
    };
    let spec = env.spec();
    let w = appliances::wiring(spec, name, m, kind, |n, o| isoloom_render::address(spec, n, o));
    svc.volumes.clear();
    for (file, inside, contents) in w.files {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/{file}"),
            contents,
        });
        svc.volumes.push(format!("./appliances/{name}/{file}:{inside}:ro"));
    }
    if let Some(own) = &d.config {
        svc.volumes.push(format!("{ROOT}/{own}:{}:ro", w.own_config));
    }
    if let (Some(fw), Some(inside)) = (&d.firmware, w.firmware) {
        svc.volumes.push(format!("{ROOT}/{fw}:{inside}:ro"));
    }
    // An emulator Isoloom builds itself (its Dockerfile next to the Compose file).
    if let Some(dockerfile) = w.build {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/build/Dockerfile"),
            contents: pull_from(env, &dockerfile),
        });
        svc.image = None;
        svc.build = Some(Build {
            context: format!("./appliances/{name}/build"),
            ..Build::default()
        });
    }
    // `$$`: the script's own variables, not Compose's interpolation.
    svc.entrypoint = w.entrypoint.into_iter().map(|e| e.replace('$', "$$")).collect();
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
fn appliance_networks(spec: &isoloom_spec::Spec, name: &str, ports: &[appliances::Port]) -> IndexMap<String, Option<Attachment>> {
    let mgmt = Attachment {
        ipv4_address: Some(appliances::mgmt_address(spec, name).to_string()),
        interface_name: Some("eth0".into()),
        priority: Some(1000),
        ..Attachment::default()
    };
    let mut nets = IndexMap::from([(appliances::MGMT_NETWORK.to_string(), Some(mgmt))]);
    for (k, p) in ports.iter().enumerate() {
        let a = Attachment {
            ipv4_address: p.address.map(|a| a.to_string()),
            interface_name: Some(format!("eth{}", k + 1)),
            priority: Some(999 - u32::try_from(k).unwrap_or(0)),
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
                let (image, tail) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(i, _)| (i, &rest[i.len()..]));
                format!("FROM {}{tail}", env.images.pull(image))
            }
            None => line.to_string(),
        })
        .collect()
}

/// The probe healthcheck and what the machine waits for before it starts.
fn readiness(env: &Environment, mp: &MachinePlan, m: &Machine, svc: &mut Service, archs: &mut Vec<Arch>) {
    let spec = env.spec();
    if !m.ready_ports().is_empty() {
        svc.healthcheck = Some(Healthcheck {
            test: vec!["CMD".into(), format!("{PROBE_DIR}/busybox"), "sh".into(), "-c".into(), probe(m)],
            interval: "5s".into(),
            timeout: "3s".into(),
            retries: 60,
            start_period: Some("10s".into()),
        });
        // Isoloom's own probe, copied into a volume before the machine starts.
        let probe_svc = probe_service(mp.arch);
        svc.volumes.push(format!("{probe_svc}:{PROBE_DIR}:ro"));
        svc.after(probe_svc, "service_completed_successfully");
        if !archs.contains(&mp.arch) {
            archs.push(mp.arch);
        }
    }
    for dep in &mp.depends_on {
        let dm = &spec.machines[dep];
        svc.after(dep.clone(), ready_condition(dm));
        for init in init_names(dep, dm) {
            svc.after(init, "service_completed_successfully");
        }
        if sidecars::needs_routes(env, dep) {
            svc.after(format!("{dep}-routes"), "service_healthy");
        }
    }
    let limits = Limits {
        cpus: mp.size.cpus.map(|c| c.to_string()),
        memory: mp.size.memory_mb.map(|mb| format!("{mb}M")),
    };
    if limits.cpus.is_some() || limits.memory.is_some() {
        svc.deploy = Some(Deploy {
            resources: Resources { limits },
        });
    }
    // What the machine serves, for tools reading the running containers (dashboards, runners):
    // `isoloom.service.<name>: "<http|tcp|udp>:<port>"` (an unnamed UDP service's key is
    // `<port>-udp`, so a port served over both keeps both).
    for sv in &m.services {
        let kind = if sv.http { "http" } else { sv.protocol.id() };
        svc.labels
            .insert(format!("isoloom.service.{}", m.service_label(sv)), format!("{kind}:{}", sv.port));
    }
}

/// One-shot init jobs, in order, each after the previous one. They share the machine's network
/// namespace (same addresses, names and routes) and its volumes, as a VM's steps see its disk.
fn init_jobs(env: &Environment, name: &str, mp: &MachinePlan, m: &Machine, services: &mut IndexMap<String, Service>) {
    let Some(d) = &m.docker else { return };
    let image = match &mp.image {
        Some(Image::Container { reference, .. }) => Some(reference.clone()),
        _ => None,
    };
    let mut previous: Option<String> = None;
    for (job, script) in init_names(name, m).into_iter().zip(&d.init) {
        let mounted = format!("/isoloom/init/{}", file_name(script));
        let mut j = Service {
            image: image.clone(),
            entrypoint: vec!["/bin/sh".into(), mounted.clone()],
            network_mode: Some(format!("service:{name}")),
            environment: environment(env, mp),
            restart: Some("no".into()),
            ..Service::default()
        };
        j.volumes.push(format!("{ROOT}/{script}:{mounted}:ro"));
        j.volumes.extend(mp.volumes.iter().map(|v| format!("{}:{}", v.key, v.path)));
        j.after(name, ready_condition(m));
        if sidecars::needs_routes(env, name) {
            j.after(format!("{name}-routes"), "service_healthy");
        }
        if let Some(p) = &previous {
            j.after(p.clone(), "service_completed_successfully");
        }
        services.insert(job.clone(), j);
        previous = Some(job);
    }
}
