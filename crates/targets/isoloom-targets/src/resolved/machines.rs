//! The snapshot's machines: each one's forms, addresses (in the spec's blocks and on Docker's),
//! ports, routes, resources and OS, as the generators work them out.

use isoloom_plan::addressing::address;
use isoloom_render::router;
use isoloom_spec::model::{Machine, Spec};
use serde_json::{Map, Value, json};

use super::ports;

/// Every machine, resolved; `docker` is the spec on Docker's blocks.
pub(super) fn machines(spec: &Spec, docker: &Spec) -> Map<String, Value> {
    spec.machines
        .iter()
        .map(|(name, m)| (name.clone(), machine(spec, docker, name, m).into()))
        .collect()
}

/// One machine, resolved.
fn machine(spec: &Spec, docker: &Spec, name: &str, m: &Machine) -> Map<String, Value> {
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
    let default_gateway = router::default_gateway(spec, name, m);
    let online = m.networks.keys().any(|n| spec.networks.get(n).is_some_and(|net| net.internet));
    let mut out = Map::new();
    out.insert("access".into(), json!(m.access));
    out.insert("supplied".into(), json!(m.supplied));
    out.insert("shapes".into(), json!(shapes));
    out.insert("role".into(), json!("target"));
    out.insert("arch".into(), json!(m.arch.id()));
    out.insert("os".into(), json!(m.vm.as_ref().map(|v| v.os.clone())));
    out.insert("windows".into(), json!(m.vm.as_ref().is_some_and(|v| isoloom_spec::images::is_windows(&v.os))));
    out.insert("addresses".into(), addresses(spec, m).into());
    out.insert("docker_addresses".into(), addresses(docker, m).into());
    out.insert("ports".into(), ports::of(m).into());
    out.insert("depends_on".into(), json!(m.depends_on));
    out.insert("inputs".into(), json!(m.inputs));
    out.insert("volumes".into(), json!(m.volumes));
    out.insert("resources".into(), resources(m));
    out.insert("gateway_of".into(), json!(gateway_of(spec, name)));
    out.insert("routes".into(), json!(routes));
    out.insert("default_gateway".into(), json!(default_gateway.map(|a| a.to_string())));
    out.insert("offline".into(), json!(!online && default_gateway.is_none()));
    add_os(&mut out, m);
    add_more_addresses(&mut out, spec, m);
    out
}

/// Each service machine's catalogue services (design §4.9, step 8): the entry, the release its
/// version picked, where it comes from, and the official image a container runs where its OS
/// has one (else the container is built from the OS's image).
pub(super) fn add_services(machines: &mut Map<String, Value>, lab: &isoloom_plan::Lab) {
    for (name, entry) in machines.iter_mut() {
        let Ok(resolved) = lab.catalogue().resolve(lab.source(), name) else {
            continue;
        };
        if resolved.is_empty() {
            continue;
        }
        let services: Vec<Value> = resolved
            .iter()
            .map(|r| {
                json!({
                    "entry": r.entry.as_str(),
                    "release": r.release.as_str(),
                    "source": r.source.id(),
                    "image": r.image.as_ref().map(|i| i.reference.clone()),
                })
            })
            .collect();
        if let Value::Object(m) = entry {
            m.insert("services".into(), Value::Array(services));
        }
    }
}

/// The machine's size, the defaults filled in.
fn resources(m: &Machine) -> Value {
    json!({
        "cpus": m.resources.and_then(|r| r.cpus).unwrap_or(isoloom_spec::DEFAULT_CPUS),
        "memory_mb": m.resources.and_then(|r| r.memory_mb).unwrap_or(isoloom_spec::DEFAULT_MEMORY_MB),
        "disk_gb": m.resources.and_then(|r| r.disk_gb).unwrap_or(isoloom_spec::DEFAULT_DISK_GB),
    })
}

/// The networks the machine is the gateway of.
fn gateway_of(spec: &Spec, name: &str) -> Vec<String> {
    spec.networks
        .iter()
        .filter(|(_, n)| n.gateway.as_deref() == Some(name))
        .map(|(n, _)| n.clone())
        .collect()
}

/// The machine's OS resolved, beside its image-table name in `os`: `os_family` and
/// `os_version`, the pinned default when the spec left the version out (`debian` is 12). From
/// its own `os`, else its `vm.os` (the image-table name parsing left there); neither on a
/// container without an `os`.
fn add_os(out: &mut Map<String, Value>, m: &Machine) {
    let os = match m.os.as_deref() {
        Some(text) => isoloom_spec::os::resolve(text).ok(),
        None => m.vm.as_ref().and_then(|v| isoloom_spec::os::by_name(&v.os)),
    };
    if let Some(os) = os {
        out.insert("os_family".into(), json!(os.family));
        out.insert("os_version".into(), json!(os.version));
        out.insert("os".into(), json!(os.name));
    }
}

/// The machine's IPv6 addresses, and its further addresses on a network (`lan: [10, 11]`,
/// after the primary one in `addresses`), when it has any.
fn add_more_addresses(out: &mut Map<String, Value>, spec: &Spec, m: &Machine) {
    let v6 = isoloom_plan::ipv6::addresses(spec, m);
    if !v6.is_empty() {
        let shown: Map<String, Value> = v6.iter().map(|(n, a)| (n.clone(), json!(a.to_string()))).collect();
        out.insert("ipv6_addresses".into(), shown.into());
    }
    let more = isoloom_plan::more_addresses::of(spec, m);
    if !more.is_empty() {
        let shown: Map<String, Value> = more
            .iter()
            .map(|(n, a)| (n.clone(), json!(a.iter().map(ToString::to_string).collect::<Vec<_>>())))
            .collect();
        out.insert("more_addresses".into(), shown.into());
    }
}

/// A machine's address on each of its networks, as `on` (the spec, or the spec on Docker) gives
/// them.
fn addresses(on: &Spec, m: &Machine) -> Map<String, Value> {
    m.networks.iter().map(|(n, o)| (n.clone(), json!(address(on, n, *o).to_string()))).collect()
}
