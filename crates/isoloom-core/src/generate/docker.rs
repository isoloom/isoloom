//! The `docker` target: one Compose file at `.isoloom/docker/compose.yml`.
//!
//! - Each network becomes a Compose network with its subnet (Docker at `.1`, or at the last
//!   address when a machine is the network's gateway); `internet: false` makes it `internal`
//!   when nothing routes between networks.
//! - A machine that is a network's gateway gets forwarding and `NET_ADMIN`; the machines
//!   behind it route everything through it (in a sidecar, once it answers).
//! - Each machine becomes a service with its fixed address on every network; services find
//!   each other by name (Compose DNS).
//! - Machines with services get a healthcheck (TCP probe), so `depends_on` waits for them
//!   to answer and `docker compose up --wait` means "everything answers". The probe is
//!   Isoloom's own static busybox (a one-shot `isoloom-probe-<arch>` copies it into a volume),
//!   so it needs nothing from the image: distroless and `scratch` machines work too.
//! - `init:` scripts run once, in one-shot containers of the machine's image on its networks,
//!   after it answers; machines depending on it wait for them to finish.
//! - Inputs become environment variables read from the shell (`${NAME:-}`), only on the
//!   machines that list them.
//! - Checks run in a `check` profile, one runner per position (`isoloom-check` where a user
//!   stands, `isoloom-check-<machine>` for the others), each a sh script in `checks/`.
//!
//! - A service's `publish` port is published on the host's loopback (127.0.0.1) only, on an
//!   ephemeral host port Docker picks, so two labs publishing the same port never collide; the
//!   runner reads the real port back from `docker compose ps`. ISOLOOM_PUBLISH_ADDRESS overrides
//!   the address and ISOLOOM_PUBLISH_FIXED pins the host port to `publish` (both set inside the
//!   docker-vm/cloud VMs, where the lab is alone and that fixed port is forwarded from loopback).
//! - `volumes:` become named volumes: they survive re-creating a container, and go with
//!   `docker compose down -v`.
//!
//! Nothing is published on the host: the environment is reached from its own networks.

use serde_yaml_ng::{Mapping, Value};

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, appliances, common_unsupported, header, router, trunks};
use crate::checks;
use crate::model::{Arch, Machine, Spec, Target};

const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
const ROOT: &str = "../..";
/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
pub(super) const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

pub(super) fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}

pub(super) fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut m = Mapping::new();
    for (k, v) in entries {
        m.insert(s(k), v);
    }
    Value::Mapping(m)
}

pub(super) fn list(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Sequence(items.into_iter().collect())
}

/// The image a machine runs: the published one, or the name its build is tagged with (so
/// its `init:` jobs can reuse it).
pub(super) fn image_of(spec: &Spec, name: &str, m: &Machine) -> String {
    let d = m.docker.as_ref().expect("docker target: every machine has docker:");
    d.image.clone().unwrap_or_else(|| format!("isoloom/{}-{}", spec.name, name))
}

/// A static busybox Isoloom brings into every machine with services, so its probe needs
/// nothing from the machine's image (distroless and `scratch` images have no shell).
pub(super) const PROBE_IMAGE: &str = "busybox:1.37.0-musl";
/// Where a machine sees that busybox (read-only).
pub(super) const PROBE_DIR: &str = "/.isoloom-probe";

/// The one-shot service (Compose) that copies the probe for machines of this architecture.
pub(super) fn probe_service(arch: Arch) -> String {
    format!("isoloom-probe-{}", arch.id())
}

/// A TCP probe for each service port, run by Isoloom's own busybox (see [`PROBE_IMAGE`]).
pub(super) fn probe(m: &Machine) -> String {
    m.services
        .iter()
        .map(|svc| format!("{PROBE_DIR}/busybox nc -z -w 2 127.0.0.1 {}", svc.port))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// The probe as an exec-form command: no shell from the image.
pub(super) fn probe_command(m: &Machine) -> Vec<Value> {
    vec![s(format!("{PROBE_DIR}/busybox")), s("sh"), s("-c"), s(probe(m))]
}

fn environment(m: &Machine) -> Option<Value> {
    if m.inputs.is_empty() {
        return None;
    }
    let mut env = Mapping::new();
    for name in &m.inputs {
        env.insert(s(name.as_str()), s(format!("${{{name}:-}}")));
    }
    Some(Value::Mapping(env))
}

fn networks_of(m: &Machine, spec: &Spec, with_address: bool) -> Value {
    let mut nets = Mapping::new();
    for (net, octet) in &m.networks {
        let v = if with_address {
            map([("ipv4_address", s(address(spec, net, *octet).to_string()))])
        } else {
            Value::Null
        };
        nets.insert(s(net.as_str()), v);
    }
    Value::Mapping(nets)
}

/// Init job service names for a machine.
fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map(|d| d.init.len()).unwrap_or(0);
    (1..=n).map(|i| format!("{name}-init-{i}")).collect()
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `spec` is the spec as laid out on Docker (networks on their Docker blocks); `original` is
/// what the author wrote, to say which networks moved.
pub fn generate(spec: &Spec, original: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    common_unsupported(spec, Target::Docker)?;

    let mut appliance_files: Vec<GeneratedFile> = Vec::new();
    let mut services = Mapping::new();
    // The architectures of machines that need the probe: one copy job (and volume) each.
    let mut probe_archs: Vec<Arch> = Vec::new();
    if router::needed(spec) {
        services.insert(s(router::NAME), router_service(spec));
    }
    // 802.1Q: a switch per LAN with trunks (see `trunks`).
    let ts = trunks::trunks(spec);
    let mut lans: Vec<&str> = ts.iter().map(|t| t.lan.as_str()).collect();
    lans.dedup();
    for lan in &lans {
        let of_lan: Vec<&trunks::Trunk> = ts.iter().filter(|t| t.lan == *lan).collect();
        services.insert(s(trunks::switch_name(lan)), switch_service(spec, &of_lan));
    }

    // Machines that get a network sidecar (routes via the router or gateways, the default
    // route through a gateway, no default route when offline), so dependents can wait for it.
    let needs_routes = |name: &str, m: &Machine| !route_commands(spec, name, m).is_empty();

    for (name, m) in &spec.machines {
        // The access machine may have no implementation: the runner supplies it.
        let Some(d) = &m.docker else { continue };
        let mut svc = Mapping::new();
        let image = image_of(spec, name, m);
        if let Some(build) = &d.build {
            svc.insert(s("build"), map([("context", s(format!("{ROOT}/{build}")))]));
        }
        svc.insert(s("image"), s(image.clone()));
        // Pin the architecture so the machine runs the same on an x86-64 or an ARM host.
        svc.insert(s("platform"), s(m.arch.docker_platform()));
        if m.privileged {
            svc.insert(s("privileged"), Value::Bool(true));
        }
        if m.read_only {
            svc.insert(s("read_only"), Value::Bool(true));
        }
        if let Some(shm) = &m.shm_size {
            svc.insert(s("shm_size"), s(shm.as_str()));
        }
        if !m.tmpfs.is_empty() {
            svc.insert(s("tmpfs"), list(m.tmpfs.iter().map(|p| s(p.as_str()))));
        }
        if let Some(dns) = &m.dns {
            if !dns.servers.is_empty() {
                svc.insert(s("dns"), list(dns.servers.iter().map(|d| s(d.as_str()))));
            }
            if !dns.search.is_empty() {
                svc.insert(s("dns_search"), list(dns.search.iter().map(|d| s(d.as_str()))));
            }
            if let Some(domain) = &dns.domain {
                svc.insert(s("domainname"), s(domain.as_str()));
            }
        }
        // A stock image the runner supplies for the user to work from, or one the spec marks
        // `idle`: kept running idle (its own command may be a shell that exits at once).
        if m.supplied || m.docker.as_ref().is_some_and(|d| d.idle) {
            svc.insert(s("entrypoint"), list([s("sleep"), s("infinity")]));
        }
        svc.insert(s("hostname"), s(name.as_str()));
        let mut nets = networks_of(m, spec, true);
        if let Value::Mapping(map) = &mut nets {
            // A VLAN it reaches through a trunk isn't an interface of its own: the trunk is.
            map.retain(|k, _| !trunks::carried(&ts, name, k.as_str().unwrap_or_default()));
            for t in trunks::of(&ts, name) {
                map.insert(s(trunks::link_network(t)), map_mac(&trunks::mac(t, 0)));
            }
        }
        svc.insert(s("networks"), nets);
        if router::is_gateway(spec, name) {
            // It routes: forwarding on, and the right to set its own firewall rules.
            svc.insert(s("cap_add"), list([s("NET_ADMIN")]));
            let mut sysctls = Mapping::new();
            sysctls.insert(s("net.ipv4.ip_forward"), s("1"));
            svc.insert(s("sysctls"), Value::Mapping(sysctls));
        }
        if let Some(hosts) = extra_hosts(spec, name) {
            svc.insert(s("extra_hosts"), hosts);
        }
        if let Some(env) = environment(m) {
            svc.insert(s("environment"), env);
        }
        let ports: Vec<Value> = m
            .services
            .iter()
            .filter_map(|svc| {
                // Ephemeral loopback host port by default (no collisions); ISOLOOM_PUBLISH_FIXED
                // pins it to `publish` inside the docker-vm/cloud VMs that forward it.
                svc.publish.map(|p| {
                    s(format!(
                        "${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:${{ISOLOOM_PUBLISH_FIXED:+{p}}}:{port}",
                        port = svc.port
                    ))
                })
            })
            .collect();
        if !ports.is_empty() {
            svc.insert(s("ports"), Value::Sequence(ports));
        }
        if !m.volumes.is_empty() {
            let mounts = m.volumes.iter().map(|(v, path)| s(format!("{}:{path}", volume_name(name, v)))).collect();
            svc.insert(s("volumes"), Value::Sequence(mounts));
        }
        // A network appliance: its files, its start, its interfaces in the order it expects.
        if let Some(kind) = d.appliance {
            let w = appliances::wiring(spec, name, m, kind, |n, o| address(spec, n, o));
            let mut mounts = Vec::new();
            for (file, inside, contents) in w.files {
                appliance_files.push(GeneratedFile {
                    path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/{file}"),
                    contents,
                });
                mounts.push(s(format!("./appliances/{name}/{file}:{inside}:ro")));
            }
            if let Some(own) = &d.config {
                mounts.push(s(format!("{ROOT}/{own}:{}:ro", w.own_config)));
            }
            if let (Some(fw), Some(inside)) = (&d.firmware, w.firmware) {
                mounts.push(s(format!("{ROOT}/{fw}:{inside}:ro")));
            }
            svc.insert(s("volumes"), Value::Sequence(mounts));
            // An emulator Isoloom builds itself (its Dockerfile next to the Compose file).
            if let Some(dockerfile) = w.build {
                appliance_files.push(GeneratedFile {
                    path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/build/Dockerfile"),
                    contents: dockerfile,
                });
                svc.remove(s("image"));
                svc.insert(s("build"), map([("context", s(format!("./appliances/{name}/build")))]));
            }
            // `$$`: the script's own variables, not Compose's interpolation.
            svc.insert(s("entrypoint"), list(w.entrypoint.into_iter().map(|e| s(e.replace('$', "$$")))));
            let mut env = Mapping::new();
            for (k, v) in w.environment {
                env.insert(s(k), s(v));
            }
            svc.insert(s("environment"), Value::Mapping(env));
            if w.privileged {
                svc.insert(s("privileged"), Value::Bool(true));
            } else {
                svc.insert(s("cap_add"), list([s("NET_ADMIN"), s("NET_RAW")]));
            }
            let mut nets = Mapping::new();
            nets.insert(
                s(appliances::MGMT_NETWORK),
                // The interface names the image expects, set outright (Compose 2.36+, Docker 28.1+):
                // the order Docker attaches networks in isn't theirs to rely on.
                map([
                    ("ipv4_address", s(appliances::mgmt_address(spec, name).to_string())),
                    ("interface_name", s("eth0")),
                    ("priority", Value::Number(1000.into())),
                ]),
            );
            for (k, (net, octet)) in m.networks.iter().enumerate() {
                nets.insert(
                    s(net.as_str()),
                    map([
                        ("ipv4_address", s(address(spec, net, *octet).to_string())),
                        ("interface_name", s(format!("eth{}", k + 1))),
                        ("priority", Value::Number((999 - k as u64).into())),
                    ]),
                );
            }
            svc.insert(s("networks"), Value::Mapping(nets));
        }
        if !m.services.is_empty() {
            svc.insert(
                s("healthcheck"),
                map([
                    ("test", list(std::iter::once(s("CMD")).chain(probe_command(m)))),
                    ("interval", s("5s")),
                    ("timeout", s("3s")),
                    ("retries", Value::Number(60.into())),
                    ("start_period", s("10s")),
                ]),
            );
        }
        let mut deps = Mapping::new();
        if !m.services.is_empty() {
            // Isoloom's own probe, copied into a volume before the machine starts.
            let probe_svc = probe_service(m.arch);
            let mount = s(format!("{probe_svc}:{PROBE_DIR}:ro"));
            match svc.get_mut(s("volumes")) {
                Some(Value::Sequence(v)) => v.push(mount),
                _ => {
                    svc.insert(s("volumes"), list([mount]));
                }
            }
            deps.insert(s(probe_svc.as_str()), map([("condition", s("service_completed_successfully"))]));
            if !probe_archs.contains(&m.arch) {
                probe_archs.push(m.arch);
            }
        }
        for dep in &m.depends_on {
            let dm = &spec.machines[dep];
            deps.insert(s(dep.as_str()), map([("condition", s("service_healthy"))]));
            for init in init_names(dep, dm) {
                deps.insert(s(init), map([("condition", s("service_completed_successfully"))]));
            }
            if needs_routes(dep, dm) {
                deps.insert(s(format!("{dep}-routes")), map([("condition", s("service_healthy"))]));
            }
        }
        if !deps.is_empty() {
            svc.insert(s("depends_on"), Value::Mapping(deps));
        }
        if let Some(r) = m.resources {
            let mut limits = Mapping::new();
            if let Some(c) = r.cpus {
                limits.insert(s("cpus"), s(c.to_string()));
            }
            if let Some(mb) = r.memory_mb {
                limits.insert(s("memory"), s(format!("{mb}M")));
            }
            if !limits.is_empty() {
                svc.insert(s("deploy"), map([("resources", map([("limits", Value::Mapping(limits))]))]));
            }
        }
        // What the machine serves, for tools reading the running containers (dashboards,
        // launchers): `isoloom.service.<name>: "<http|tcp>:<port>"`.
        if !m.services.is_empty() {
            let mut labels = Mapping::new();
            for sv in &m.services {
                let key = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                let kind = if sv.http { "http" } else { "tcp" };
                labels.insert(s(format!("isoloom.service.{key}")), s(format!("{kind}:{}", sv.port)));
            }
            svc.insert(s("labels"), Value::Mapping(labels));
        }
        svc.insert(s("restart"), s("unless-stopped"));
        services.insert(s(name.as_str()), Value::Mapping(svc));

        if needs_routes(name, m) {
            services.insert(s(format!("{name}-routes")), routes_sidecar(spec, name, name, m));
        }

        // One-shot init jobs, in order, each after the previous one. They share the machine's
        // network namespace: same addresses, names and routes.
        let mut previous: Option<String> = None;
        for (job, script) in init_names(name, m).into_iter().zip(&d.init) {
            let mut j = Mapping::new();
            j.insert(s("image"), s(image.clone()));
            let mounted = format!("/isoloom/init/{}", file_name(script));
            j.insert(s("entrypoint"), list([s("/bin/sh"), s(mounted.clone())]));
            // The job runs "in" the machine: its volumes too, as a VM's steps see its disk.
            let mut mounts = vec![s(format!("{ROOT}/{script}:{mounted}:ro"))];
            mounts.extend(m.volumes.iter().map(|(v, path)| s(format!("{}:{path}", volume_name(name, v)))));
            j.insert(s("volumes"), Value::Sequence(mounts));
            j.insert(s("network_mode"), s(format!("service:{name}")));
            if let Some(env) = environment(m) {
                j.insert(s("environment"), env);
            }
            let mut deps = Mapping::new();
            deps.insert(s(name.as_str()), map([("condition", s("service_healthy"))]));
            if needs_routes(name, m) {
                deps.insert(s(format!("{name}-routes")), map([("condition", s("service_healthy"))]));
            }
            if let Some(p) = &previous {
                deps.insert(s(p.as_str()), map([("condition", s("service_completed_successfully"))]));
            }
            j.insert(s("depends_on"), Value::Mapping(deps));
            j.insert(s("restart"), s("no"));
            services.insert(s(job.as_str()), Value::Mapping(j));
            previous = Some(job);
        }
    }

    // The check runners: one per position. A machine's checks run in its network namespace (a
    // stand-in's when the runner supplies the access machine); checks without a position stand
    // on every network. Each runner is a sh script next to the Compose file.
    let plan = checks::plan(spec);
    let default_pos = checks::default_position(spec);
    let mut runner_files = Vec::new();
    let host = |h: &checks::Host, _: &checks::Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
        }
    };
    let run_script = |path: &str| format!("cd /isoloom/project && sh {path}");
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    let mut stand_in_added = false;
    for (pos, group) in checks::by_position(spec, &plan) {
        let id = pos.id();
        let runner = if pos == default_pos {
            "isoloom-check".to_string()
        } else {
            format!("isoloom-check-{id}")
        };
        runner_files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/{id}.sh"),
            contents: checks::script(&pos, &group, &render),
        });
        let mut volumes = vec![s(format!("./checks/{id}.sh:/isoloom/run.sh:ro"))];
        if group.iter().any(|c| matches!(c.probe, checks::Probe::Script { .. })) {
            volumes.push(s(format!("{ROOT}:/isoloom/project:ro")));
        }
        // After every machine answers (and its init jobs and routes are done).
        let mut deps = Mapping::new();
        for (name, m) in &spec.machines {
            if m.docker.is_none() {
                continue;
            }
            if !m.services.is_empty() {
                deps.insert(s(name.as_str()), map([("condition", s("service_healthy"))]));
            }
            for init in init_names(name, m) {
                deps.insert(s(init), map([("condition", s("service_completed_successfully"))]));
            }
            if needs_routes(name, m) {
                deps.insert(s(format!("{name}-routes")), map([("condition", s("service_healthy"))]));
            }
        }
        let mut c = Mapping::new();
        c.insert(s("image"), s(CHECK_IMAGE));
        c.insert(s("profiles"), list([s("check")]));
        c.insert(s("entrypoint"), list([s("/bin/sh"), s("/isoloom/run.sh")]));
        c.insert(s("volumes"), Value::Sequence(volumes));
        match &pos {
            checks::Position::Machine(name) => {
                let m = &spec.machines[name];
                let ns = if m.docker.is_some() {
                    if m.services.is_empty() {
                        deps.insert(s(name.as_str()), map([("condition", s("service_started"))]));
                    }
                    name.clone()
                } else {
                    if !stand_in_added {
                        services.insert(s(STAND_IN), stand_in(spec, name, m));
                        if needs_routes(name, m) {
                            let mut sidecar = routes_sidecar(spec, STAND_IN, name, m);
                            if let Value::Mapping(map) = &mut sidecar {
                                // Like the stand-in, only started with the check profile.
                                map.insert(s("profiles"), list([s("check")]));
                            }
                            services.insert(s(format!("{STAND_IN}-routes")), sidecar);
                        }
                        stand_in_added = true;
                    }
                    deps.insert(s(STAND_IN), map([("condition", s("service_started"))]));
                    if needs_routes(name, m) {
                        deps.insert(s(format!("{STAND_IN}-routes")), map([("condition", s("service_healthy"))]));
                    }
                    STAND_IN.to_string()
                };
                c.insert(s("network_mode"), s(format!("service:{ns}")));
            }
            checks::Position::Networks => {
                let mut nets = Mapping::new();
                for net in spec.networks.keys() {
                    nets.insert(s(net.as_str()), Value::Null);
                }
                c.insert(s("networks"), Value::Mapping(nets));
                // Standing on offline networks that aren't Docker-internal, the runner drops its
                // default route like the machines do (root, to change its own routes).
                let offline = !spec.networks.values().any(|n| n.internet) && spec.networks.keys().any(|n| !internal(spec, n));
                if offline {
                    c.insert(s("user"), s("0"));
                    c.insert(s("cap_add"), list([s("NET_ADMIN")]));
                    c.insert(
                        s("entrypoint"),
                        list([s("/bin/sh"), s("-c"), s("ip route del default 2>/dev/null; exec sh /isoloom/run.sh")]),
                    );
                }
            }
        }
        if !deps.is_empty() {
            c.insert(s("depends_on"), Value::Mapping(deps));
        }
        services.insert(s(runner), Value::Mapping(c));
    }

    // Tools: observers on every network at the reserved addresses, outside the environment's
    // contract (no reach rule, no check names them).
    for (i, (name, tool)) in spec.tools.iter().enumerate() {
        let mut t = Mapping::new();
        let shell = name == "shell";
        t.insert(s("image"), s(tool.image.clone().unwrap_or_else(|| TOOLBOX_IMAGE.to_string())));
        t.insert(s("hostname"), s(name.as_str()));
        if !tool.command.is_empty() {
            t.insert(s("command"), list(tool.command.iter().map(|c| s(c.as_str()))));
        } else if shell {
            t.insert(s("entrypoint"), list([s("sleep"), s("infinity")]));
        }
        if shell {
            t.insert(s("cap_add"), list([s("NET_ADMIN"), s("NET_RAW")]));
        }
        let mut nets = Mapping::new();
        for net in spec.networks.keys() {
            let cidr = crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
            nets.insert(s(net.as_str()), map([("ipv4_address", s(cidr.tool(i).to_string()))]));
        }
        t.insert(s("networks"), Value::Mapping(nets));
        if let (Some(port), Some(host)) = (tool.port, tool.publish) {
            t.insert(s("ports"), list([s(format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{host}:{port}"))]));
        }
        t.insert(s("restart"), s("unless-stopped"));
        services.insert(s(format!("isoloom-tool-{name}")), Value::Mapping(t));
    }

    let mut networks = Mapping::new();
    for (net, n) in &spec.networks {
        let gateway = super::host_address(spec, net);
        let mut v = Mapping::new();
        v.insert(
            s("ipam"),
            map([("config", list([map([("subnet", s(n.cidr.as_str())), ("gateway", s(gateway.to_string()))])]))]),
        );
        // Without routing, `internet: false` is Docker's internal network. With a router or a
        // gateway, Docker's internal-network firewall would drop forwarded traffic, so offline
        // machines lose their default route instead (in their sidecar). A network with a
        // gateway is never internal: its gateway decides what leaves it. Nor is a network a
        // machine publishes a port from (Docker can't publish from internal networks).
        if internal(spec, net) {
            v.insert(s("internal"), Value::Bool(true));
        }
        networks.insert(s(net.as_str()), Value::Mapping(v));
    }

    for t in &ts {
        networks.insert(s(trunks::link_network(t)), map([("internal", Value::Bool(true))]));
    }
    if !appliances::appliances(spec).is_empty() {
        networks.insert(
            s(appliances::MGMT_NETWORK),
            map([("ipam", map([("config", list([map([("subnet", s(appliances::MGMT_CIDR))])]))]))]),
        );
    }

    let mut volumes = Mapping::new();
    for (name, m) in &spec.machines {
        if m.docker.is_some() {
            for v in m.volumes.keys() {
                volumes.insert(s(volume_name(name, v)), Value::Mapping(Mapping::new()));
            }
        }
    }

    for arch in &probe_archs {
        let name = probe_service(*arch);
        let mut p = Mapping::new();
        p.insert(s("image"), s(PROBE_IMAGE));
        p.insert(s("platform"), s(arch.docker_platform()));
        p.insert(s("entrypoint"), list([s("/bin/cp"), s("/bin/busybox"), s("/probe/busybox")]));
        p.insert(s("volumes"), list([s(format!("{name}:/probe"))]));
        p.insert(s("network_mode"), s("none"));
        p.insert(s("restart"), s("no"));
        services.insert(s(name.as_str()), Value::Mapping(p));
        volumes.insert(s(name.as_str()), Value::Mapping(Mapping::new()));
    }

    let mut root = Mapping::new();
    root.insert(s("name"), s(spec.name.as_str()));
    root.insert(s("services"), Value::Mapping(services));
    root.insert(s("networks"), Value::Mapping(networks));
    if !volumes.is_empty() {
        root.insert(s("volumes"), Value::Mapping(volumes));
    }
    let yaml = serde_yaml_ng::to_string(&Value::Mapping(root)).expect("a compose mapping serializes");

    let moved: String = spec
        .networks
        .iter()
        .filter(|(n, net)| original.networks[*n].cidr != net.cidr)
        .map(|(n, net)| {
            format!(
                "# On Docker, network `{n}` uses {} instead of {} (same last octets).\n",
                net.cidr, original.networks[n].cidr
            )
        })
        .collect();
    let usage = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: isoloom test docker (or: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check, and isoloom-check-<machine>)\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{usage}{moved}\n{yaml}", header("#")),
    }];
    files.extend(runner_files);
    files.extend(appliance_files);
    Ok(files)
}

/// A machine's volume, named for Compose (scoped to the environment by Compose itself).
fn volume_name(machine: &str, volume: &str) -> String {
    format!("{machine}-{volume}")
}

/// The `shell` tool's image: a toolbox with tcpdump, nmap, curl, dig, netcat and more.
pub(super) const TOOLBOX_IMAGE: &str = "nicolaka/netshoot";

/// Stands in for an access machine the runner supplies, so checks run from its side.
const STAND_IN: &str = "isoloom-access";
/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
pub(super) const UTILITY_IMAGE: &str = "alpine:3.20";

/// Names of machines that share no network with `name` (Compose DNS only resolves
/// machines on shared networks); they're reached through the router.
fn extra_hosts(spec: &Spec, name: &str) -> Option<Value> {
    let m = &spec.machines[name];
    let ts = trunks::trunks(spec);
    let hosts: Vec<Value> = spec
        .machines
        .iter()
        // Docker's own names only reach across a network both are attached to: not one a trunk
        // carries.
        .filter(|(o, om)| {
            o.as_str() != name
                && !om
                    .networks
                    .keys()
                    .any(|n| m.networks.contains_key(n) && !trunks::carried(&ts, name, n) && !trunks::carried(&ts, o, n))
        })
        .map(|(o, _)| s(format!("{o}:{}", address_for(spec, name, o))))
        .collect();
    (!hosts.is_empty()).then_some(Value::Sequence(hosts))
}

/// A network attachment with a fixed MAC address (a trunk's end, found by it).
fn map_mac(mac: &str) -> Value {
    map([("mac_address", s(mac))])
}

/// A LAN's switch: on each of its VLAN networks and on each trunk to it, bridging them per VLAN.
fn switch_service(spec: &Spec, trunks: &[&trunks::Trunk]) -> Value {
    let mut nets = Mapping::new();
    for t in trunks {
        for (net, _, _) in &t.vlans {
            nets.insert(s(net.as_str()), map([("ipv4_address", s(trunks::switch_address(spec, net).to_string()))]));
        }
        nets.insert(s(trunks::link_network(t)), map_mac(&trunks::mac(t, 1)));
    }
    let mut r = Mapping::new();
    r.insert(s("image"), s(TC_IMAGE));
    r.insert(s("cap_add"), list([s("NET_ADMIN")]));
    r.insert(s("networks"), Value::Mapping(nets));
    r.insert(
        s("entrypoint"),
        list([s("/bin/sh"), s("-c"), s(trunks::switch_script(spec, trunks).replace('$', "$$"))]),
    );
    r.insert(
        s("healthcheck"),
        map([
            ("test", list([s("CMD-SHELL"), s(trunks::switch_ready(trunks))])),
            ("interval", s("2s")),
            ("timeout", s("2s")),
            ("retries", Value::Number(30.into())),
        ]),
    );
    r.insert(s("restart"), s("unless-stopped"));
    Value::Mapping(r)
}

/// The router: on every network at its last address, forwarding with the `reach` rules.
fn router_service(spec: &Spec) -> Value {
    let mut nets = Mapping::new();
    for net in router::networks(spec) {
        nets.insert(s(net.as_str()), map([("ipv4_address", s(router::address(spec, net).to_string()))]));
    }
    let mut r = Mapping::new();
    r.insert(s("image"), s(UTILITY_IMAGE));
    r.insert(s("hostname"), s(router::NAME));
    r.insert(s("cap_add"), list([s("NET_ADMIN")]));
    let mut sysctls = Mapping::new();
    sysctls.insert(s("net.ipv4.ip_forward"), s("1"));
    r.insert(s("sysctls"), Value::Mapping(sysctls));
    r.insert(s("networks"), Value::Mapping(nets));
    let mut env = Mapping::new();
    env.insert(s("RULES"), s(router::nftables(spec)));
    let tc = router::tc_script(spec);
    if let Some(t) = &tc {
        // `$$`: the script's own variables, not Compose's interpolation.
        env.insert(s("TC"), s(t.replace('$', "$$")));
    }
    r.insert(s("environment"), Value::Mapping(env));
    // `$$` keeps Compose from interpolating the shell variable. With `tc`, iproute2 (for tc) and
    // the netem commands after the rules.
    let start = if tc.is_some() {
        "apk add --no-cache nftables iproute2 >/dev/null && printf '%s' \"$$RULES\" | nft -f - && printf '%s' \"$$TC\" | sh && exec sleep infinity"
    } else {
        "apk add --no-cache nftables >/dev/null && printf '%s' \"$$RULES\" | nft -f - && exec sleep infinity"
    };
    r.insert(s("entrypoint"), list([s("/bin/sh"), s("-c"), s(start)]));
    r.insert(
        s("healthcheck"),
        map([
            ("test", list([s("CMD-SHELL"), s("nft list table inet isoloom >/dev/null 2>&1")])),
            ("interval", s("3s")),
            ("timeout", s("3s")),
            ("retries", Value::Number(60.into())),
        ]),
    );
    r.insert(s("restart"), s("unless-stopped"));
    Value::Mapping(r)
}

/// The commands a machine's sidecar runs: routes via the router and gateways, the default
/// route through its gateway (or, for a gateway, out through Docker on a network with
/// internet), and no default route when the machine is offline.
fn route_commands(spec: &Spec, name: &str, m: &Machine) -> Vec<String> {
    // An appliance routes itself (IOS): its sidecar only hands the data addresses over to it.
    if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) {
        return appliances::sidecar_commands(m);
    }
    // Its trunks first: the routes below may go through addresses only they carry.
    let ts = trunks::trunks(spec);
    let mut cmds: Vec<String> = trunks::of(&ts, name)
        .flat_map(|t| trunks::machine_commands(spec, m, t, |n, o| address(spec, n, o), |n| super::host_address(spec, n)))
        .map(|c| c.replace('$', "$$"))
        .collect();
    cmds.extend(router::route_commands(spec, name, m, true));
    if let Some(out) = own_default(spec, name, m) {
        cmds.push(format!("ip route replace default via {out}"));
    }
    if offline(spec, name, m) && m.networks.keys().any(|n| !internal(spec, n)) {
        cmds.push("(ip route del default 2>/dev/null || true)".into());
    }
    // `tc` on its own interfaces, one subshell per line (`$$`: not Compose's interpolation).
    if let Some(script) = router::machine_tc_script(spec, m, |n, o| address(spec, n, o)) {
        cmds.extend(script.lines().map(|l| format!("({})", l.replace('$', "$$"))));
    }
    cmds
}

/// The sidecar image of a machine on a network with `tc`: one with `tc` (the utility image has
/// none, and an offline machine couldn't install it).
const TC_IMAGE: &str = "nicolaka/netshoot:v0.13";

/// Whether a machine's sidecar needs `tc` or 802.1Q (`ip link ... type vlan`): on a network
/// with `tc`, or on a trunk.
fn impaired(spec: &Spec, m: &Machine) -> bool {
    m.networks.keys().any(|n| spec.networks[n].tc.is_some())
        || m.networks.keys().filter(|n| spec.networks[*n].vlan.is_some()).count() >= 2 && m.docker.is_some()
}

/// Whether a network is Docker's internal network: offline, nothing routes, and no machine
/// on it publishes a port. Otherwise its offline machines lose their default route instead.
fn internal(spec: &Spec, network: &str) -> bool {
    let n = &spec.networks[network];
    let publishes = spec
        .machines
        .values()
        .any(|m| m.docker.is_some() && m.networks.contains_key(network) && m.services.iter().any(|svc| svc.publish.is_some()));
    !n.internet && !router::routed(spec) && !publishes
}

/// A gateway's way out: Docker's address on its first network with internet that the
/// router (or nothing) routes, so Docker doesn't pick one of the networks it serves.
fn own_default(spec: &Spec, name: &str, m: &Machine) -> Option<std::net::Ipv4Addr> {
    if !router::is_gateway(spec, name) || router::default_gateway(spec, name, m).is_some() {
        return None;
    }
    m.networks
        .keys()
        .find(|n| router::plain(spec, n) && spec.networks[*n].internet)
        .map(|n| super::host_address(spec, n))
}

/// A container in `host`'s network namespace that sets the routes of machine `name`.
fn routes_sidecar(spec: &Spec, host: &str, name: &str, m: &Machine) -> Value {
    let mut r = Mapping::new();
    r.insert(s("image"), s(if impaired(spec, m) { TC_IMAGE } else { UTILITY_IMAGE }));
    r.insert(s("network_mode"), s(format!("service:{host}")));
    r.insert(s("cap_add"), list([s("NET_ADMIN")]));
    // Sets the routes, then stays (idle) so the healthcheck can confirm them and `up --wait`
    // treats it as running rather than exited.
    let cmds = route_commands(spec, name, m);
    r.insert(
        s("entrypoint"),
        list([s("/bin/sh"), s("-c"), s(format!("{} && exec sleep infinity", cmds.join(" && ")))]),
    );
    let mut ready = Vec::new();
    if let Some((first, _)) = router::routes(spec, name, m).first() {
        ready.push(format!("ip route show {first} | grep -q via"));
    }
    if let Some(gw) = router::default_gateway(spec, name, m).or_else(|| own_default(spec, name, m)) {
        ready.push(format!("ip route | grep -q '^default via {gw} '"));
    }
    if offline(spec, name, m) && m.networks.keys().any(|n| !internal(spec, n)) {
        ready.push("! ip route | grep -q '^default'".into());
    }
    if m.networks.keys().any(|n| spec.networks[n].tc.is_some()) {
        ready.push("tc qdisc show | grep -q netem".into());
    }
    let ts = trunks::trunks(spec);
    for t in trunks::of(&ts, name) {
        ready.push(trunks::machine_ready(t));
    }
    // An appliance's sidecar only hands its data addresses over: ready once they're gone.
    if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) {
        ready = vec!["! ip -o -4 addr show | grep -q ' eth[1-9]'".into()];
    }
    r.insert(
        s("healthcheck"),
        map([
            ("test", list([s("CMD-SHELL"), s(ready.join(" && "))])),
            ("interval", s("2s")),
            ("timeout", s("2s")),
            ("retries", Value::Number(30.into())),
        ]),
    );
    let mut deps = Mapping::new();
    deps.insert(s(host), map([("condition", s("service_started"))]));
    for t in trunks::of(&ts, name) {
        deps.insert(s(trunks::switch_name(&t.lan)), map([("condition", s("service_healthy"))]));
    }
    if router::needed(spec) && m.networks.keys().any(|n| router::plain(spec, n)) {
        deps.insert(s(router::NAME), map([("condition", s("service_healthy"))]));
    }
    // The gateways it routes through, ready (answering, so their rules are loaded).
    for gw in crate::validate::starts_after(spec, name, m) {
        if m.depends_on.iter().any(|d| d == gw) || !router::is_gateway(spec, gw) || spec.machines[gw].docker.is_none() {
            continue;
        }
        let ready = if spec.machines[gw].services.is_empty() {
            "service_started"
        } else {
            "service_healthy"
        };
        deps.insert(s(gw), map([("condition", s(ready))]));
    }
    r.insert(s("depends_on"), Value::Mapping(deps));
    r.insert(s("restart"), s("unless-stopped"));
    Value::Mapping(r)
}

/// An idle container at the access machine's addresses, for the check runner to stand in.
fn stand_in(spec: &Spec, name: &str, m: &Machine) -> Value {
    let mut a = Mapping::new();
    a.insert(s("image"), s(UTILITY_IMAGE));
    a.insert(s("hostname"), s(name));
    a.insert(s("networks"), networks_of(m, spec, true));
    if let Some(hosts) = extra_hosts(spec, name) {
        a.insert(s("extra_hosts"), hosts);
    }
    a.insert(s("entrypoint"), list([s("sleep"), s("infinity")]));
    a.insert(s("profiles"), list([s("check")]));
    Value::Mapping(a)
}

/// A machine is offline when none of its networks reaches the internet and no gateway
/// decides for it.
pub(super) fn offline(spec: &Spec, name: &str, m: &Machine) -> bool {
    !m.networks.keys().any(|n| spec.networks[n].internet) && router::default_gateway(spec, name, m).is_none()
}
