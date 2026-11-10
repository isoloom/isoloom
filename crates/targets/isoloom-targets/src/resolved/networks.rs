//! The snapshot's networks, and the addresses Isoloom gives its own infrastructure on them: the
//! router, the controller and the tools.

use isoloom_render::router;
use isoloom_spec::model::Spec;
use isoloom_spec::net::NetworkBlock;
use isoloom_spec::net::docker_cidrs;
use serde_json::{Map, Value, json};

/// One address per network of the spec, `at` its block. A network whose block isn't a
/// network's (a spec that isn't valid) has no address, nor one where `at` gives none.
fn per_network(spec: &Spec, at: impl Fn(NetworkBlock) -> Option<String>) -> Map<String, Value> {
    spec.networks
        .iter()
        .filter_map(|(name, n)| Some((name.clone(), json!(at(NetworkBlock::parse(&n.cidr)?)?))))
        .collect()
}

/// Each network with its blocks and the addresses Isoloom gives its infrastructure (`routed`:
/// the spec needs the router).
pub(super) fn networks(spec: &Spec, routed: bool) -> Map<String, Value> {
    let docker_blocks: Map<String, Value> = docker_cidrs(spec).into_iter().map(|(n, c)| (n, json!(c.to_string()))).collect();
    let mut networks = Map::new();
    for (name, n) in &spec.networks {
        let Some(c) = NetworkBlock::parse(&n.cidr) else { continue };
        let mut network = Map::new();
        network.insert("cidr".into(), json!(n.cidr));
        network.insert("docker_cidr".into(), docker_blocks.get(name).cloned().unwrap_or(Value::Null));
        network.insert("internet".into(), json!(n.internet));
        network.insert("gateway".into(), json!(n.gateway));
        network.insert("gateway_address".into(), json!(c.gateway().to_string()));
        network.insert("router".into(), json!((routed && router::plain(spec, name)).then(|| c.router().to_string())));
        network.insert("controller".into(), json!(c.controller().to_string()));
        if let Some(v6) = &n.ipv6 {
            network.insert("ipv6".into(), json!(v6));
        }
        networks.insert(name.clone(), Value::Object(network));
    }
    networks
}

/// The tools, with their addresses on every network (in the spec's blocks and on Docker's).
pub(super) fn tools(spec: &Spec, docker: &Spec) -> Map<String, Value> {
    let on_docker = isoloom_plan::tools::of(docker);
    isoloom_plan::tools::of(spec)
        .into_iter()
        .map(|(n, t)| {
            let docker_addresses = on_docker.get(&n).map(|d| &d.addresses);
            let tool = json!({
                "image": t.image,
                "port": t.tool.port,
                "publish": t.tool.publish,
                "addresses": t.addresses,
                "docker_addresses": docker_addresses,
            });
            (n, tool)
        })
        .collect()
}

/// The router, when the spec needs one: its networks and its address on each.
pub(super) fn router(spec: &Spec, routed: bool) -> Option<Value> {
    routed.then(|| {
        let nets: Vec<&String> = router::networks(spec).collect();
        json!({
            "networks": nets,
            "addresses": nets.iter().map(|n| ((*n).clone(), json!(router::address(spec, n).to_string()))).collect::<Map<_, _>>(),
        })
    })
}

/// The controller: where environment-level provisioning runs, and its address on each network.
pub(super) fn controller(spec: &Spec) -> Value {
    json!({
        "role": "infra",
        "keep_running": spec.controller.as_ref().is_some_and(|c| c.keep_running),
        "addresses": per_network(spec, |c| Some(c.controller().to_string())),
    })
}
