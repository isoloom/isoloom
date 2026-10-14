//! The Compose file of a planned [`Environment`]: a service per container machine (with its
//! `init:` jobs and network sidecar), Isoloom's router, VLAN switches, probes, check runners and
//! tools, then the networks and volumes; every resource labelled as the environment's.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Feature, Image, MachinePlan, MachineShape, PublishedPort};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, constraints, header, trunks};
use isoloom_spec::model::{Arch, Machine, Target};

use crate::appliances;
use crate::docker::{
    DIR, ENVIRONMENT_LABEL, MANAGED_LABEL, PROBE_DIR, PROBE_IMAGE, ROOT, TOOLBOX_IMAGE, file_name, init_names, probe, probe_service, ready_condition,
    relative_to,
};
use crate::file::{Attachment, Build, ComposeFile, Deploy, Healthcheck, Ipam, Labelled, Limits, Network, Resources, Service, Subnet, Ulimit};
use crate::sidecars;

/// What Compose can render: network appliances (wired the way Docker attaches networks), aliases,
/// gateway machines, ulimits and `tc` (in a sidecar). No controller: no `provision:`, no Ansible
/// checks; `exec` checks run inside a container.
pub fn capabilities() -> isoloom_plan::Capabilities {
    isoloom_plan::Capabilities::of([
        Feature::NetworkAppliances,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
    ])
}

/// The Compose backend of the `docker` and `hosted` targets (a hosting service runs the same
/// Compose file).
#[derive(Debug, Clone, Copy)]
pub struct ComposeBackend(pub Target);

impl Backend for ComposeBackend {
    fn target(&self) -> Target {
        self.0
    }

    fn capabilities(&self) -> isoloom_plan::Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render(self.0, env)
    }
}

/// The Compose file of `env` and the files it mounts (check runners, appliance configurations),
/// refused as `target` when `env` needs what Compose can't do.
pub fn render(target: Target, env: &Environment) -> Result<Files, RenderError> {
    let Rendered { file, files: mounted } = compose_file(target, env)?;
    let yaml = to_yaml(target, &file)?;
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{USAGE}{}\n{yaml}", header("#"), moved(env)),
    }];
    files.extend(mounted);
    Ok(files)
}

/// A Compose file before it is written, and the files it mounts.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// The Compose file.
    pub file: ComposeFile,
    /// The files next to it: check runners, appliance configurations, Dockerfiles.
    pub files: Files,
}

/// The typed Compose file of `env` and the files it mounts, for a target that adapts it before
/// writing it (`hybrid` plugs it into its Docker host's networks). Refused as `target` when
/// `env` needs what Compose can't do.
pub fn compose_file(target: Target, env: &Environment) -> Result<Rendered, RenderError> {
    backend::check(target, env, &capabilities())?;
    let mut out = Out::default();
    let ts = trunks::trunks(env.spec());
    sidecars::router_and_switches(env, &ts, &mut out.services);
    for (name, mp) in &env.machines {
        if mp.shape == MachineShape::Container {
            machine(env, name, mp, &ts, &mut out);
        }
    }
    let runners = sidecars::check_runners(env, &mut out.services);
    tools(env, &mut out.services);
    let mut file = ComposeFile {
        name: env.name.clone(),
        networks: networks(env, &ts),
        volumes: volumes(env, &out.probe_archs),
        ..ComposeFile::default()
    };
    probes(env, &out.probe_archs, &mut out.services);
    file.services = out.services;
    label(env, &mut file);
    let mut files = runners;
    files.extend(out.files);
    Ok(Rendered { file, files })
}

/// `file` as YAML, a failure reported as `target`'s.
pub fn to_yaml(target: Target, file: &ComposeFile) -> Result<String, RenderError> {
    serde_yaml_ng::to_string(file).map_err(|e| RenderError::Generate(isoloom_render::GenerateError::Unsupported { target, what: e.to_string() }))
}

const USAGE: &str = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: isoloom test docker (or: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check, and isoloom-check-<machine>)\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";

/// What rendering the machines collects besides their services.
#[derive(Default)]
struct Out {
    services: IndexMap<String, Service>,
    /// Appliance configurations and Dockerfiles, next to the Compose file.
    files: Vec<GeneratedFile>,
    /// The architectures of machines that need the probe: one copy job (and volume) each.
    probe_archs: Vec<Arch>,
}

/// The comment saying which networks moved to their Docker block.
fn moved(env: &Environment) -> String {
    env.networks
        .iter()
        .filter(|(_, n)| n.declared_cidr != n.cidr)
        .map(|(name, n)| {
            format!(
                "# On Docker, network `{name}` uses {} instead of {} (same last octets).\n",
                n.cidr, n.declared_cidr
            )
        })
        .collect()
}

/// A machine's service, then its sidecar and its `init:` jobs.
fn machine(env: &Environment, name: &str, mp: &MachinePlan, ts: &[trunks::Trunk], out: &mut Out) {
    let m = &env.spec().machines[name];
    let mut svc = base(env, name, mp, m, ts);
    if let Some(kind) = m.docker.as_ref().and_then(|d| d.appliance) {
        appliance(env, name, m, kind, &mut svc, &mut out.files);
    }
    readiness(env, name, mp, m, &mut svc, &mut out.probe_archs);
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
fn port(p: PublishedPort) -> String {
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

/// A network appliance: its files, its start, its interfaces in the order it expects.
#[allow(clippy::too_many_arguments)] // Tracked: the arguments become one struct (Wave 2).
fn appliance(env: &Environment, name: &str, m: &Machine, kind: isoloom_spec::model::Appliance, svc: &mut Service, files: &mut Vec<GeneratedFile>) {
    let spec = env.spec();
    let d = m.docker.as_ref().expect("an appliance is a container");
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
#[allow(clippy::too_many_arguments)] // Tracked: the arguments become one struct (Wave 2).
fn readiness(env: &Environment, name: &str, mp: &MachinePlan, m: &Machine, svc: &mut Service, archs: &mut Vec<Arch>) {
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
    let _ = name;
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
            restart: Some("unless-stopped".into()),
            ..Service::default()
        };
        if tool.command.is_empty() && shell {
            t.entrypoint = vec!["sleep".into(), "infinity".into()];
        }
        if shell {
            t.cap_add = vec!["NET_ADMIN".into(), "NET_RAW".into()];
        }
        for net in spec.networks.keys() {
            let cidr = isoloom_spec::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
            t.networks.insert(net.clone(), Attachment::at(cidr.tool(i)));
        }
        if let (Some(port), Some(host)) = (tool.port, tool.publish) {
            t.ports = vec![format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{host}:{port}")];
        }
        services.insert(format!("isoloom-tool-{name}"), t);
    }
}

/// The one-shot services that copy the probe, one per architecture.
fn probes(env: &Environment, archs: &[Arch], services: &mut IndexMap<String, Service>) {
    for arch in archs {
        let name = probe_service(*arch);
        let p = Service {
            image: Some(env.images.pull(PROBE_IMAGE)),
            platform: Some(arch.docker_platform().to_string()),
            entrypoint: vec!["/bin/cp".into(), "/bin/busybox".into(), "/probe/busybox".into()],
            volumes: vec![format!("{name}:/probe")],
            network_mode: Some("none".into()),
            restart: Some("no".into()),
            ..Service::default()
        };
        services.insert(name, p);
    }
}

/// The networks: each on its block, Docker at the host's address. Without routing,
/// `internet: false` is Docker's internal network; then the trunks' links and the appliances'
/// management network.
fn networks(env: &Environment, ts: &[trunks::Trunk]) -> IndexMap<String, Network> {
    let mut out: IndexMap<String, Network> = env
        .networks
        .iter()
        .map(|(name, n)| {
            let subnet = Subnet {
                subnet: n.cidr.clone(),
                gateway: Some(n.host.to_string()),
            };
            let net = Network {
                ipam: Some(Ipam { config: vec![subnet] }),
                internal: sidecars::internal(env, name),
                ..Network::default()
            };
            (name.clone(), net)
        })
        .collect();
    for t in ts {
        out.insert(
            trunks::link_network(t),
            Network {
                internal: true,
                ..Network::default()
            },
        );
    }
    if !appliances::appliances(env.spec()).is_empty() {
        let subnet = Subnet {
            subnet: appliances::MGMT_CIDR.into(),
            gateway: None,
        };
        out.insert(
            appliances::MGMT_NETWORK.into(),
            Network {
                ipam: Some(Ipam { config: vec![subnet] }),
                ..Network::default()
            },
        );
    }
    out
}

/// The named volumes: the machines', then the probes'.
fn volumes(env: &Environment, archs: &[Arch]) -> IndexMap<String, Labelled> {
    let own = env
        .machines
        .values()
        .filter(|m| m.shape == MachineShape::Container)
        .flat_map(|m| m.volumes.iter().map(|v| v.key.clone()));
    own.chain(archs.iter().map(|a| probe_service(*a))).map(|k| (k, Labelled::default())).collect()
}

/// Everything the environment creates says so (containers, built images, networks, volumes),
/// so leftovers can be told apart and collected (`isoloom gc`) without guessing.
fn label(env: &Environment, file: &mut ComposeFile) {
    let ours = [
        (MANAGED_LABEL.to_string(), "true".to_string()),
        (ENVIRONMENT_LABEL.to_string(), env.name.clone()),
    ];
    for svc in file.services.values_mut() {
        svc.labels.extend(ours.clone());
        if let Some(b) = &mut svc.build {
            b.labels.extend(ours.clone());
        }
    }
    for n in file.networks.values_mut() {
        n.labels.extend(ours.clone());
    }
    for v in file.volumes.values_mut() {
        v.labels.extend(ours.clone());
    }
}
