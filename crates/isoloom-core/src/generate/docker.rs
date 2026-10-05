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
//!   to answer and `docker compose up --wait` means "everything answers".
//! - `init:` scripts run once, in one-shot containers of the machine's image on its networks,
//!   after it answers; machines depending on it wait for them to finish.
//! - Inputs become environment variables read from the shell (`${NAME:-}`), only on the
//!   machines that list them.
//! - Checks run in a `check` profile: `docker compose --profile check run --rm isoloom-check`.
//!
//! - A service's `publish` port is published on the host's loopback (127.0.0.1) only, unless
//!   ISOLOOM_PUBLISH_ADDRESS says otherwise (inside the docker-vm target's VM, which is itself
//!   only forwarded from the host's loopback).
//! - `volumes:` become named volumes: they survive re-creating a container, and go with
//!   `docker compose down -v`.
//!
//! Nothing is published on the host: the environment is reached from its own networks.

use serde_yaml_ng::{Mapping, Value};

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, header, router};
use crate::model::{Machine, Spec, Target};

const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
const ROOT: &str = "../..";
/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}

fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut m = Mapping::new();
    for (k, v) in entries {
        m.insert(s(k), v);
    }
    Value::Mapping(m)
}

fn list(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Sequence(items.into_iter().collect())
}

/// The image a machine runs: the published one, or the name its build is tagged with (so
/// its `init:` jobs can reuse it).
fn image_of(spec: &Spec, name: &str, m: &Machine) -> String {
    let d = m.docker.as_ref().expect("docker target: every machine has docker:");
    d.image.clone().unwrap_or_else(|| format!("isoloom/{}-{}", spec.name, name))
}

/// A TCP probe for each service port that works in most images: busybox/BSD `nc`, else bash.
fn probe(m: &Machine) -> String {
    m.services
        .iter()
        .map(|svc| {
            format!(
                "(nc -z 127.0.0.1 {p} 2>/dev/null || bash -c '</dev/tcp/127.0.0.1/{p}' 2>/dev/null)",
                p = svc.port
            )
        })
        .collect::<Vec<_>>()
        .join(" && ")
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

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `spec` is the spec as laid out on Docker (networks on their Docker blocks); `original` is
/// what the author wrote, to say which networks moved.
pub fn generate(spec: &Spec, original: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    common_unsupported(spec, Target::Docker)?;

    let mut services = Mapping::new();
    if router::needed(spec) {
        services.insert(s(router::NAME), router_service(spec));
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
        svc.insert(s("hostname"), s(name.as_str()));
        svc.insert(s("networks"), networks_of(m, spec, true));
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
            .filter_map(|svc| svc.publish.map(|p| s(format!("${{ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}}:{p}:{}", svc.port))))
            .collect();
        if !ports.is_empty() {
            svc.insert(s("ports"), Value::Sequence(ports));
        }
        if !m.volumes.is_empty() {
            let mounts = m.volumes.iter().map(|(v, path)| s(format!("{}:{path}", volume_name(name, v)))).collect();
            svc.insert(s("volumes"), Value::Sequence(mounts));
        }
        if !m.services.is_empty() {
            svc.insert(
                s("healthcheck"),
                map([
                    ("test", list([s("CMD-SHELL"), s(probe(m))])),
                    ("interval", s("5s")),
                    ("timeout", s("3s")),
                    ("retries", Value::Number(60.into())),
                    ("start_period", s("10s")),
                ]),
            );
        }
        let mut deps = Mapping::new();
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

    // The check runner stands where a user would: in the access machine's network namespace
    // (a stand-in when the runner supplies the access machine), else on every network.
    if !spec.checks.is_empty() {
        let mut volumes = Vec::new();
        let mut run = Vec::new();
        for (i, c) in spec.checks.iter().enumerate() {
            let mounted = format!("/isoloom/checks/{:02}-{}", i + 1, file_name(c));
            volumes.push(s(format!("{ROOT}/{c}:{mounted}:ro")));
            run.push(format!("echo '== {c}' && sh {mounted}"));
        }
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
        c.insert(s("entrypoint"), list([s("/bin/sh"), s("-c"), s(run.join(" && "))]));
        c.insert(s("volumes"), Value::Sequence(volumes));
        match spec.machines.iter().find(|(_, m)| m.access) {
            Some((name, m)) => {
                let host = if m.docker.is_some() {
                    name.clone()
                } else {
                    services.insert(s(STAND_IN), stand_in(spec, name, m));
                    deps.insert(s(STAND_IN), map([("condition", s("service_started"))]));
                    if needs_routes(name, m) {
                        let mut sidecar = routes_sidecar(spec, STAND_IN, name, m);
                        if let Value::Mapping(map) = &mut sidecar {
                            // Like the stand-in, only started with the check profile.
                            map.insert(s("profiles"), list([s("check")]));
                        }
                        services.insert(s(format!("{STAND_IN}-routes")), sidecar);
                        deps.insert(s(format!("{STAND_IN}-routes")), map([("condition", s("service_healthy"))]));
                    }
                    STAND_IN.to_string()
                };
                c.insert(s("network_mode"), s(format!("service:{host}")));
            }
            None => {
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
                        list([s("/bin/sh"), s("-c"), s(format!("ip route del default 2>/dev/null; {}", run.join(" && ")))]),
                    );
                }
            }
        }
        if !deps.is_empty() {
            c.insert(s("depends_on"), Value::Mapping(deps));
        }
        services.insert(s("isoloom-check"), Value::Mapping(c));
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

    let mut volumes = Mapping::new();
    for (name, m) in &spec.machines {
        if m.docker.is_some() {
            for v in m.volumes.keys() {
                volumes.insert(s(volume_name(name, v)), Value::Mapping(Mapping::new()));
            }
        }
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
    let usage = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";
    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{usage}{moved}\n{yaml}", header("#")),
    }])
}

/// A machine's volume, named for Compose (scoped to the environment by Compose itself).
fn volume_name(machine: &str, volume: &str) -> String {
    format!("{machine}-{volume}")
}

/// Stands in for an access machine the runner supplies, so checks run from its side.
const STAND_IN: &str = "isoloom-access";
/// A small image with busybox `ip`, for the router, route sidecars and the stand-in.
const UTILITY_IMAGE: &str = "alpine:3.20";

/// Names of machines that share no network with `name` (Compose DNS only resolves
/// machines on shared networks); they're reached through the router.
fn extra_hosts(spec: &Spec, name: &str) -> Option<Value> {
    let m = &spec.machines[name];
    let hosts: Vec<Value> = spec
        .machines
        .iter()
        .filter(|(o, om)| o.as_str() != name && !om.networks.keys().any(|n| m.networks.contains_key(n)))
        .map(|(o, _)| s(format!("{o}:{}", address_for(spec, name, o))))
        .collect();
    (!hosts.is_empty()).then_some(Value::Sequence(hosts))
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
    r.insert(s("environment"), Value::Mapping(env));
    // `$$` keeps Compose from interpolating the shell variable.
    r.insert(
        s("entrypoint"),
        list([
            s("/bin/sh"),
            s("-c"),
            s("apk add --no-cache nftables >/dev/null && printf '%s' \"$$RULES\" | nft -f - && exec sleep infinity"),
        ]),
    );
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
    let mut cmds = router::route_commands(spec, name, m, true);
    if let Some(out) = own_default(spec, name, m) {
        cmds.push(format!("ip route replace default via {out}"));
    }
    if offline(spec, name, m) && m.networks.keys().any(|n| !internal(spec, n)) {
        cmds.push("(ip route del default 2>/dev/null || true)".into());
    }
    cmds
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
    r.insert(s("image"), s(UTILITY_IMAGE));
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
fn offline(spec: &Spec, name: &str, m: &Machine) -> bool {
    !m.networks.keys().any(|n| spec.networks[n].internet) && router::default_gateway(spec, name, m).is_none()
}
