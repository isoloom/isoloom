//! The resolved snapshot, `.isoloom/resolved.json`: the spec after Isoloom has worked
//! everything out, for programs. Every address (machines on each network, and on Docker's
//! blocks; the router, the controller, the gateways), routes, start order, published ports,
//! the targets (possible, generated, refused with reasons), and the checks by position with
//! their runners. The generators and this file use the same functions, and a test keeps them
//! in agreement; tools embedding Isoloom read this instead of parsing a Vagrantfile.

use serde_json::{Map, Value, json};

use super::{GENERATED_TARGETS, GeneratedFile, address, docker_cidrs, on_docker, refusal, router, start_order};
use crate::checks::{self, HttpExpect, Position, Probe, Resolved, TcpExpect};
use crate::images;
use crate::model::{Machine, Protocol, Service, Spec, Target};
use crate::targets::{effective, missing, mixed};
use crate::validate::Cidr;

/// The format of the snapshot; bumped when a field changes meaning or goes away.
pub const RESOLVED_VERSION: u32 = 1;

/// Where the snapshot is written, relative to the project folder.
pub const PATH: &str = ".isoloom/resolved.json";

/// The snapshot as a generated file.
pub fn file(spec: &Spec) -> GeneratedFile {
    file_with(spec, None)
}

/// The snapshot of an instance of the spec (the spec already as that instance).
pub fn file_with(spec: &Spec, instance: Option<u8>) -> GeneratedFile {
    let mut text = serde_json::to_string_pretty(&resolve_with(spec, instance)).expect("a JSON object serializes");
    text.push('\n');
    GeneratedFile {
        path: PATH.to_string(),
        contents: text,
    }
}

/// The snapshot as JSON (validated specs only).
pub fn resolve(spec: &Spec) -> Value {
    resolve_with(spec, None)
}

/// The snapshot of an instance (`None`: the spec as written).
pub fn resolve_with(spec: &Spec, instance: Option<u8>) -> Value {
    let docker = on_docker(spec);
    let routed = router::needed(spec);
    let mut out = json!({
        "generated_by_isoloom": "from isoloom.yml; don't edit: change the spec and run `isoloom generate` (`isoloom check` fails when this file is out of date)",
        "resolved_version": RESOLVED_VERSION,
        "name": spec.name,
        "instance": instance,
        "inputs": spec.inputs,
        "networks": networks(spec, routed),
        "reach": spec.reach.iter().map(|r| json!({ "from": r.from, "to": r.to, "ports": r.ports })).collect::<Vec<_>>(),
        "machines": machines(spec, &docker),
        "start_order": start_order(spec),
        "clones": spec.clones,
        "tools": tools(spec, &docker),
        "groups": spec.groups.keys().map(|g| (g.clone(), json!(crate::groups::members(spec, g)))).collect::<Map<_, _>>(),
        "router": router_value(spec, routed),
        "controller": controller(spec),
        "published": published(spec),
        "targets": targets(spec),
        "checks": checks_section(spec),
    });
    if let Value::Object(map) = &mut out {
        add_cloud(spec, map);
    }
    // The message, with its placeholders filled from everything above (a cloud-services
    // message keeps its `{{ cloud.outputs.* }}`: they are only known once deployed).
    if let Some(m) = &spec.message {
        let filled = fill(m, &out).ok().or_else(|| spec.cloud.as_ref().map(|_| m.clone()));
        if let Value::Object(map) = &mut out {
            map.insert("message".into(), filled.map(Value::String).unwrap_or(Value::Null));
        }
    }
    out
}

/// A network's block (validated specs only).
fn cidr(spec: &Spec, net: &str) -> Cidr {
    Cidr::parse(&spec.networks[net].cidr).expect("validated cidr")
}

/// One address per network of the spec, from its block.
fn per_network(spec: &Spec, at: impl Fn(&str) -> String) -> Map<String, Value> {
    spec.networks.keys().map(|n| (n.clone(), json!(at(n)))).collect()
}

/// Each network with its blocks and the addresses Isoloom gives its infrastructure (`routed`:
/// the spec needs the router).
fn networks(spec: &Spec, routed: bool) -> Map<String, Value> {
    let docker_blocks: Map<String, Value> = docker_cidrs(spec)
        .into_iter()
        .map(|(n, c)| (n, json!(format!("{}/{}", std::net::Ipv4Addr::from(c.base), c.len))))
        .collect();
    let mut networks = Map::new();
    for (name, n) in &spec.networks {
        let c = cidr(spec, name);
        networks.insert(
            name.clone(),
            json!({
                "cidr": n.cidr,
                "docker_cidr": docker_blocks[name],
                "internet": n.internet,
                "gateway": n.gateway,
                "gateway_address": c.gateway().to_string(),
                "router": (routed && router::plain(spec, name)).then(|| c.router().to_string()),
                "controller": c.controller().to_string(),
            }),
        );
    }
    networks
}

/// Every machine, resolved.
fn machines(spec: &Spec, docker: &Spec) -> Map<String, Value> {
    spec.machines.iter().map(|(name, m)| (name.clone(), machine(spec, docker, name, m))).collect()
}

fn machine(spec: &Spec, docker: &Spec, name: &str, m: &Machine) -> Value {
    let mut shapes = Vec::new();
    if m.docker.is_some() {
        shapes.push("docker");
    }
    if m.vm.is_some() {
        shapes.push("vm");
    }
    let routes: Vec<Value> = router::routes(spec, name, m)
        .into_iter()
        .map(|(to, via)| json!({ "to": to, "via": via.to_string() }))
        .collect();
    json!({
        "access": m.access,
        "supplied": m.supplied,
        "shapes": shapes,
        "role": "target",
        "arch": m.arch.id(),
        "os": m.vm.as_ref().map(|v| v.os.clone()),
        "windows": m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)),
        "addresses": addresses(spec, m),
        "docker_addresses": addresses(docker, m),
        "services": m.services.iter().map(service).collect::<Vec<_>>(),
        "depends_on": m.depends_on,
        "inputs": m.inputs,
        "volumes": m.volumes,
        "resources": {
            "cpus": m.resources.and_then(|r| r.cpus).unwrap_or(isoloom_spec::DEFAULT_CPUS),
            "memory_mb": m.resources.and_then(|r| r.memory_mb).unwrap_or(isoloom_spec::DEFAULT_MEMORY_MB),
            "disk_gb": m.resources.and_then(|r| r.disk_gb).unwrap_or(isoloom_spec::DEFAULT_DISK_GB),
        },
        "gateway_of": spec.networks.iter().filter(|(_, n)| n.gateway.as_deref() == Some(name)).map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        "routes": routes,
        "default_gateway": router::default_gateway(spec, name, m).map(|a| a.to_string()),
        "offline": !m.networks.keys().any(|n| spec.networks[n].internet) && router::default_gateway(spec, name, m).is_none(),
    })
}

/// A machine's address on each of its networks, as `on` (the spec, or the spec on Docker) gives them.
fn addresses(on: &Spec, m: &Machine) -> Map<String, Value> {
    m.networks.iter().map(|(n, o)| (n.clone(), json!(address(on, n, *o).to_string()))).collect()
}

/// A service; the flags only when set, the protocol only when not TCP.
fn service(s: &Service) -> Value {
    let mut v = json!({ "port": s.port, "name": s.name, "http": s.http, "publish": s.publish });
    for (key, set) in [("tls", s.tls), ("fixed", s.fixed), ("dormant", s.dormant)] {
        if set {
            v[key] = json!(true);
        }
    }
    with_protocol(v, s.protocol)
}

/// `v` with a `protocol` field when it isn't TCP (a snapshot entry without one is TCP).
fn with_protocol(mut v: Value, protocol: Protocol) -> Value {
    if !protocol.is_tcp() {
        v["protocol"] = json!(protocol.id());
    }
    v
}

/// The tools, with their addresses on every network (in the spec's blocks and on Docker's).
fn tools(spec: &Spec, docker: &Spec) -> Map<String, Value> {
    spec.tools
        .iter()
        .enumerate()
        .map(|(i, (n, t))| {
            (
                n.clone(),
                json!({
                    "image": t.image.clone().or_else(|| (n == "shell").then(|| "nicolaka/netshoot".to_string())),
                    "port": t.port,
                    "publish": t.publish,
                    "addresses": per_network(spec, |net| cidr(spec, net).tool(i).to_string()),
                    "docker_addresses": per_network(spec, |net| cidr(docker, net).tool(i).to_string()),
                }),
            )
        })
        .collect()
}

/// The router, when the spec needs one: its networks and its address on each.
fn router_value(spec: &Spec, routed: bool) -> Option<Value> {
    routed.then(|| {
        let nets: Vec<&String> = router::networks(spec).collect();
        json!({
            "networks": nets,
            "addresses": nets.iter().map(|n| ((*n).clone(), json!(router::address(spec, n).to_string()))).collect::<Map<_, _>>(),
        })
    })
}

fn controller(spec: &Spec) -> Value {
    json!({
        "role": "infra",
        "keep_running": spec.controller.as_ref().is_some_and(|c| c.keep_running),
        "addresses": per_network(spec, |n| cidr(spec, n).controller().to_string()),
    })
}

/// Every published service: its machine, port and host port.
fn published(spec: &Spec) -> Vec<Value> {
    spec.machines
        .iter()
        .flat_map(|(name, m)| {
            m.services.iter().filter_map(move |s| {
                s.publish
                    .map(|host| with_protocol(json!({ "machine": name, "port": s.port, "host_port": host }), s.protocol))
            })
        })
        .collect()
}

/// Targets: possible by the implementations, narrowed by `targets:`, then what the
/// generators actually produce and why they refuse the rest.
fn targets(spec: &Spec) -> Value {
    let possible = effective(spec);
    let mut generated = Vec::new();
    let mut refused = Map::new();
    let mut not_possible = Map::new();
    for t in Target::ALL {
        if !possible.contains(&t) {
            not_possible.insert(t.id().into(), json!(not_possible_reason(spec, t)));
        } else if !GENERATED_TARGETS.contains(&t) {
            refused.insert(t.id().into(), json!("no generator yet"));
        } else if let Some(why) = refusal(spec, t) {
            refused.insert(t.id().into(), json!(why));
        } else {
            generated.push(t.id());
        }
    }
    json!({
        "possible": possible.iter().map(|t| t.id()).collect::<Vec<_>>(),
        "generated": generated,
        "refused": refused,
        "not_possible": not_possible,
    })
}

/// Why a target isn't possible for the spec.
fn not_possible_reason(spec: &Spec, t: Target) -> String {
    let lacking = missing(spec, t.needs());
    if !lacking.is_empty() {
        format!("needs `{}:` on {}", t.needs().key(), lacking.join(", "))
    } else if t == Target::Hybrid && !mixed(spec) {
        "only when some machines are containers and others are VMs".to_string()
    } else {
        "left out by `targets:`".to_string()
    }
}

/// Checks, by position, with the runner each target names them by.
fn checks_section(spec: &Spec) -> Value {
    let plan = checks::plan(spec);
    let default_pos = checks::default_position(spec);
    let positions: Vec<Value> = checks::by_position(spec, &plan)
        .into_iter()
        .map(|(pos, group)| {
            let runner = if pos == default_pos {
                "isoloom-check".to_string()
            } else {
                format!("isoloom-check-{}", pos.id())
            };
            json!({
                "position": pos.id(),
                "machine": match &pos { Position::Machine(m) => Some(m.clone()), Position::Networks => None },
                "runner": runner,
                "checks": group.iter().map(|c| check(spec, c)).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "default_position": default_pos.id(),
        "positions": positions,
    })
}

fn check(spec: &Spec, c: &Resolved) -> Value {
    let (kind, target, expect) = probe_parts(spec, &c.probe);
    let mut v = json!({ "name": c.name, "derived": c.derived, "kind": kind, "target": target, "expect": expect, "wait": c.wait });
    if let Probe::Http { request, .. } = &c.probe
        && request.follow
    {
        v["follow"] = json!(true);
    }
    v
}

/// A probe as (kind, target, expect), its hosts written as addresses in the spec's blocks.
fn probe_parts(spec: &Spec, probe: &Probe) -> (&'static str, String, String) {
    match probe {
        Probe::Http { url, expect, .. } => (
            "http",
            url.render(&host_address(spec, &url.host)),
            match expect {
                HttpExpect::Status(s) => s.to_string(),
                HttpExpect::Any => "any".into(),
                HttpExpect::Blocked => "blocked".into(),
            },
        ),
        Probe::Tcp { host, port, expect, .. } => (
            "tcp",
            format!("{}:{port}", host_address(spec, host)),
            match expect {
                TcpExpect::Open => "open".into(),
                TcpExpect::Blocked => "blocked".into(),
            },
        ),
        Probe::Exec { command, expect } => ("exec", command.clone(), expect.clone().unwrap_or_default()),
        Probe::Script { path } => ("script", path.clone(), String::new()),
        Probe::Playbook { path } => ("playbook", path.clone(), String::new()),
    }
}

fn host_address(spec: &Spec, h: &checks::Host) -> String {
    match h {
        checks::Host::Literal(l) => l.clone(),
        checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
    }
}

/// Cloud services: what tools reading the snapshot need to run them (the cloud, the module,
/// what they expose and cost). Not the variables, which may take a user's input.
fn add_cloud(spec: &Spec, map: &mut Map<String, Value>) {
    if let Some(c) = &spec.cloud {
        map.insert(
            "cloud".into(),
            json!({
                "provider": c.provider.id(),
                "terraform": c.terraform,
                "outputs": c.outputs.keys().collect::<Vec<_>>(),
                "inputs": c.vars.values().filter_map(crate::model::CloudServices::input_of).collect::<Vec<_>>(),
                "hourly_usd": c.hourly_usd,
            }),
        );
    }
}

/// The spec's `message` with its `{{ path }}` placeholders filled from the snapshot; an error
/// names a placeholder that points at nothing.
pub fn render_message(spec: &Spec, instance: Option<u8>) -> Result<Option<String>, String> {
    let Some(m) = &spec.message else { return Ok(None) };
    let snapshot = resolve_with(spec, instance);
    fill(m, &snapshot).map(Some)
}

/// The message as it reads where the environment runs on `target`: on the targets that run
/// the Compose file, machines are at their Docker addresses, so `machines.<m>.addresses` is
/// filled with those (see [`on_target`]).
pub fn render_message_on(spec: &Spec, instance: Option<u8>, target: Target) -> Result<Option<String>, String> {
    render_message_at(spec, instance, target, &[])
}

/// [`render_message_on`] with the host ports the environment really got, as (machine, port,
/// protocol, host port): local Docker publishes on free ports unless `ISOLOOM_PUBLISH_FIXED` is set, so
/// `{{ machines.web.services.0.publish }}` must say where it answers (see [`with_published`]).
pub fn render_message_at(spec: &Spec, instance: Option<u8>, target: Target, published: &[(String, u16, Protocol, u16)]) -> Result<Option<String>, String> {
    let Some(m) = &spec.message else { return Ok(None) };
    fill(m, &with_published(on_target(resolve_with(spec, instance), target), published)).map(Some)
}

/// The snapshot with the host ports the environment really got, as (machine, port, protocol,
/// host port): each service's `publish` and the `published` list's `host_port`.
pub fn with_published(mut snapshot: Value, published: &[(String, u16, Protocol, u16)]) -> Value {
    // A service without `protocol` in the snapshot is TCP.
    let same = |v: &Value, port: u16, proto: Protocol| v["port"] == port && v["protocol"].as_str().unwrap_or("tcp") == proto.id();
    for (machine, port, proto, host) in published {
        let services = snapshot.pointer_mut(&format!("/machines/{machine}/services")).and_then(Value::as_array_mut);
        for s in services.into_iter().flatten().filter(|s| same(s, *port, *proto)) {
            s["publish"] = (*host).into();
        }
        let listed = snapshot.get_mut("published").and_then(Value::as_array_mut);
        for p in listed
            .into_iter()
            .flatten()
            .filter(|p| p["machine"] == machine.as_str() && same(p, *port, *proto))
        {
            p["host_port"] = (*host).into();
        }
    }
    snapshot
}

/// The snapshot as seen on `target`: on the Compose targets (Docker, a hosting service, Docker
/// on a VM or a cloud VM) each machine's `addresses` are its `docker_addresses`, the ones it
/// really has there (they differ when the spec's networks are outside 10.0.0.0/8, or for an
/// instance); elsewhere the snapshot is unchanged.
pub fn on_target(mut snapshot: Value, target: Target) -> Value {
    if !matches!(target, Target::Docker | Target::Hosted | Target::CloudDocker | Target::DockerVm) {
        return snapshot;
    }
    if let Some(machines) = snapshot.get_mut("machines").and_then(Value::as_object_mut) {
        for m in machines.values_mut() {
            if let Some(docker) = m.get("docker_addresses").cloned() {
                m["addresses"] = docker;
            }
        }
    }
    snapshot
}

/// Fills `{{ dotted.path }}` placeholders from a snapshot. Scalars print plainly; lists and
/// maps as compact JSON.
pub fn fill(text: &str, snapshot: &Value) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err("message: a `{{` without its `}}`".into());
        };
        let path = after[..end].trim();
        let value = lookup(snapshot, path).ok_or_else(|| format!("message: `{{{{ {path} }}}}` points at nothing in the snapshot (see `isoloom inspect`)"))?;
        match value {
            Value::String(s) => out.push_str(s),
            Value::Null => {}
            other => out.push_str(&other.to_string()),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// A dotted path into the snapshot (`machines.web.addresses`, `checks.positions.0.runner`).
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|s| !s.is_empty()).try_fold(value, |v, key| match v {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}
