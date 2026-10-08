//! The resolved snapshot, `.isoloom/resolved.json`: the spec after Isoloom has worked
//! everything out, for programs. Every address (machines on each network, and on Docker's
//! blocks; the router, the controller, the gateways), routes, start order, published ports,
//! the targets (possible, generated, refused with reasons), and the checks by position with
//! their runners. The generators and this file use the same functions, and a test keeps them
//! in agreement; tools embedding Isoloom read this instead of parsing a Vagrantfile.

use serde_json::{Map, Value, json};

use super::{GENERATED_TARGETS, GeneratedFile, address, docker_cidrs, on_docker, refusal, router, start_order};
use crate::checks::{self, HttpExpect, Position, Probe, TcpExpect};
use crate::images;
use crate::model::{Spec, Target};
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
    let docker_blocks: Map<String, Value> = docker_cidrs(spec)
        .into_iter()
        .map(|(n, c)| (n, json!(format!("{}/{}", std::net::Ipv4Addr::from(c.base), c.len))))
        .collect();
    let cidr = |net: &str| Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let routed = router::needed(spec);

    let mut networks = Map::new();
    for (name, n) in &spec.networks {
        let c = cidr(name);
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

    let mut machines = Map::new();
    for (name, m) in &spec.machines {
        let addresses: Map<String, Value> = m.networks.iter().map(|(n, o)| (n.clone(), json!(address(spec, n, *o).to_string()))).collect();
        let docker_addresses: Map<String, Value> = m
            .networks
            .iter()
            .map(|(n, o)| (n.clone(), json!(address(&docker, n, *o).to_string())))
            .collect();
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
        machines.insert(
            name.clone(),
            json!({
                "access": m.access,
                "supplied": m.supplied,
                "shapes": shapes,
                "arch": m.arch.id(),
                "os": m.vm.as_ref().map(|v| v.os.clone()),
                "windows": m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)),
                "addresses": addresses,
                "docker_addresses": docker_addresses,
                "services": m.services.iter().map(|s| json!({ "port": s.port, "name": s.name, "http": s.http, "publish": s.publish })).collect::<Vec<_>>(),
                "depends_on": m.depends_on,
                "inputs": m.inputs,
                "volumes": m.volumes,
                "resources": {
                    "cpus": m.resources.and_then(|r| r.cpus).unwrap_or(crate::DEFAULT_CPUS),
                    "memory_mb": m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB),
                    "disk_gb": m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB),
                },
                "gateway_of": spec.networks.iter().filter(|(_, n)| n.gateway.as_deref() == Some(name)).map(|(n, _)| n.clone()).collect::<Vec<_>>(),
                "routes": routes,
                "default_gateway": router::default_gateway(spec, name, m).map(|a| a.to_string()),
                "offline": !m.networks.keys().any(|n| spec.networks[n].internet) && router::default_gateway(spec, name, m).is_none(),
            }),
        );
    }

    let router_value = routed.then(|| {
        let nets: Vec<&String> = router::networks(spec).collect();
        json!({
            "networks": nets,
            "addresses": nets.iter().map(|n| ((*n).clone(), json!(router::address(spec, n).to_string()))).collect::<Map<_, _>>(),
        })
    });
    let controller = json!({
        "addresses": spec.networks.keys().map(|n| (n.clone(), json!(cidr(n).controller().to_string()))).collect::<Map<_, _>>(),
    });

    let published: Vec<Value> = spec
        .machines
        .iter()
        .flat_map(|(name, m)| {
            m.services
                .iter()
                .filter_map(move |s| s.publish.map(|host| json!({ "machine": name, "port": s.port, "host_port": host })))
        })
        .collect();

    // Targets: possible by the implementations, narrowed by `targets:`, then what the
    // generators actually produce and why they refuse the rest.
    let possible = effective(spec);
    let mut generated = Vec::new();
    let mut refused = Map::new();
    let mut not_possible = Map::new();
    for t in Target::ALL {
        if possible.contains(&t) {
            if !GENERATED_TARGETS.contains(&t) {
                refused.insert(t.id().into(), json!("no generator yet"));
            } else if let Some(why) = refusal(spec, t) {
                refused.insert(t.id().into(), json!(why));
            } else {
                generated.push(t.id());
            }
        } else {
            let lacking = missing(spec, t.needs());
            let why = if !lacking.is_empty() {
                format!("needs `{}:` on {}", t.needs().key(), lacking.join(", "))
            } else if t == Target::Hybrid && !mixed(spec) {
                "only when some machines are containers and others are VMs".to_string()
            } else {
                "left out by `targets:`".to_string()
            };
            not_possible.insert(t.id().into(), json!(why));
        }
    }

    // Checks, by position, with the runner each target names them by.
    let plan = checks::plan(spec);
    let default_pos = checks::default_position(spec);
    let host = |h: &checks::Host| match h {
        checks::Host::Literal(l) => l.clone(),
        checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
    };
    let positions: Vec<Value> = checks::by_position(spec, &plan)
        .into_iter()
        .map(|(pos, group)| {
            let runner = if pos == default_pos {
                "isoloom-check".to_string()
            } else {
                format!("isoloom-check-{}", pos.id())
            };
            let list: Vec<Value> = group
                .iter()
                .map(|c| {
                    let (kind, target, expect) = match &c.probe {
                        Probe::Http { url, expect, .. } => (
                            "http",
                            url.render(&host(&url.host)),
                            match expect {
                                HttpExpect::Status(s) => s.to_string(),
                                HttpExpect::Any => "any".into(),
                                HttpExpect::Blocked => "blocked".into(),
                            },
                        ),
                        Probe::Tcp { host: h, port, expect } => (
                            "tcp",
                            format!("{}:{port}", host(h)),
                            match expect {
                                TcpExpect::Open => "open".into(),
                                TcpExpect::Blocked => "blocked".into(),
                            },
                        ),
                        Probe::Exec { command, expect } => ("exec", command.clone(), expect.clone().unwrap_or_default()),
                        Probe::Script { path } => ("script", path.clone(), String::new()),
                        Probe::Playbook { path } => ("playbook", path.clone(), String::new()),
                    };
                    json!({ "name": c.name, "derived": c.derived, "kind": kind, "target": target, "expect": expect, "wait": c.wait })
                })
                .collect();
            json!({
                "position": pos.id(),
                "machine": match &pos { Position::Machine(m) => Some(m.clone()), Position::Networks => None },
                "runner": runner,
                "checks": list,
            })
        })
        .collect();

    let mut out = json!({
        "generated_by_isoloom": "from isoloom.yml; don't edit: change the spec and run `isoloom generate` (`isoloom check` fails when this file is out of date)",
        "resolved_version": RESOLVED_VERSION,
        "name": spec.name,
        "instance": instance,
        "inputs": spec.inputs,
        "networks": networks,
        "reach": spec.reach.iter().map(|r| json!({ "from": r.from, "to": r.to, "ports": r.ports })).collect::<Vec<_>>(),
        "machines": machines,
        "start_order": start_order(spec),
        "clones": spec.clones,
        "tools": spec.tools.iter().enumerate().map(|(i, (n, t))| {
            (n.clone(), json!({
                "image": t.image.clone().or_else(|| (n == "shell").then(|| "nicolaka/netshoot".to_string())),
                "port": t.port,
                "publish": t.publish,
                "addresses": spec.networks.keys().map(|net| (net.clone(), json!(cidr(net).tool(i).to_string()))).collect::<Map<_, _>>(),
                "docker_addresses": spec.networks.keys().map(|net| (net.clone(), json!(Cidr::parse(&docker.networks[net].cidr).expect("validated cidr").tool(i).to_string()))).collect::<Map<_, _>>(),
            }))
        }).collect::<Map<_, _>>(),
        "groups": spec.groups.keys().map(|g| (g.clone(), json!(crate::groups::members(spec, g)))).collect::<Map<_, _>>(),
        "router": router_value,
        "controller": controller,
        "published": published,
        "targets": {
            "possible": possible.iter().map(|t| t.id()).collect::<Vec<_>>(),
            "generated": generated,
            "refused": refused,
            "not_possible": not_possible,
        },
        "checks": {
            "default_position": default_pos.id(),
            "positions": positions,
        },
    });
    // The message, with its placeholders filled from everything above.
    if let Some(m) = &spec.message {
        let filled = fill(m, &out).ok();
        if let Value::Object(map) = &mut out {
            map.insert("message".into(), filled.map(Value::String).unwrap_or(Value::Null));
        }
    }
    out
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
/// host port): local Docker publishes on free ports unless `ISOLOOM_PUBLISH_FIXED` is set, so
/// `{{ machines.web.services.0.publish }}` must say where it answers (see [`with_published`]).
pub fn render_message_at(spec: &Spec, instance: Option<u8>, target: Target, published: &[(String, u16, u16)]) -> Result<Option<String>, String> {
    let Some(m) = &spec.message else { return Ok(None) };
    fill(m, &with_published(on_target(resolve_with(spec, instance), target), published)).map(Some)
}

/// The snapshot with the host ports the environment really got, as (machine, port, host
/// port): each service's `publish` and the `published` list's `host_port`.
pub fn with_published(mut snapshot: Value, published: &[(String, u16, u16)]) -> Value {
    for (machine, port, host) in published {
        let services = snapshot.pointer_mut(&format!("/machines/{machine}/services")).and_then(Value::as_array_mut);
        for s in services.into_iter().flatten().filter(|s| s["port"] == *port) {
            s["publish"] = (*host).into();
        }
        let listed = snapshot.get_mut("published").and_then(Value::as_array_mut);
        for p in listed.into_iter().flatten().filter(|p| p["machine"] == machine.as_str() && p["port"] == *port) {
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
