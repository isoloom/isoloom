//! The containers' Compose file of the `hybrid` target: the `docker` target's, for the container
//! machines alone, adapted to run on the Docker host VM the Vagrant backend adds. Its networks are the macvlan networks the host creates
//! (external), the router is the router VM, the checks run from the VM side, and a container
//! reaches the VMs by name through `extra_hosts` (Docker's DNS only knows containers).

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape, PublishedPort};
use isoloom_render::backend::{RenderError, as_target};
use isoloom_render::container::{HybridContainers, docker_net};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, router};
use isoloom_spec::model::{Protocol, Spec, Target};

use crate::file::{ComposeFile, Network, Service};

/// The bridge network of the containers with published ports.
const PUBLISH_NET: &str = "isoloom-publish";

/// The Compose file of `env`'s containers, with what the Docker host needs to run it.
pub fn render(env: &Environment) -> Result<HybridContainers, RenderError> {
    let alone = Environment::build(&containers_only(env.spec()), Target::Hybrid, env.instance);
    let mut compose = crate::compose_file(Target::Docker, &alone).map_err(|e| as_target(e, Target::Hybrid))?.file;
    compose.services.shift_remove(router::NAME);
    let names: Vec<String> = compose.services.keys().cloned().collect();
    let vm_hosts = vm_hosts(env);
    let mut published = Vec::new();
    for (name, svc) in &mut compose.services {
        let ports = ports_of(&alone, name);
        if !ports.is_empty() {
            publish(svc, &ports, &mut published);
        }
        svc.depends_on.retain(|k, _| names.contains(k));
        if !svc.networks.is_empty() {
            svc.extra_hosts.extend(vm_hosts.iter().cloned());
        }
    }
    let networks: Vec<String> = env
        .networks
        .keys()
        .filter(|n| {
            env.machines
                .values()
                .any(|m| m.shape == MachineShape::Container && m.addresses.contains_key(*n))
        })
        .cloned()
        .collect();
    compose.networks = external_networks(env, &networks, !published.is_empty());
    Ok(HybridContainers {
        file: GeneratedFile {
            path: format!("{OUTPUT_DIR}/hybrid/compose.yml"),
            contents: contents(&compose)?,
        },
        published,
        networks,
    })
}

/// The spec of the container machines alone, waiting only for each other, with no checks and
/// no provisioning (both run from the VM side).
fn containers_only(spec: &Spec) -> Spec {
    let mut s = spec.clone();
    s.machines.retain(|_, m| m.docker.is_some());
    let kept: Vec<String> = s.machines.keys().cloned().collect();
    for m in s.machines.values_mut() {
        m.depends_on.retain(|d| kept.contains(d));
    }
    s.checks.clear();
    s.provision.clear();
    s
}

/// The VMs (and supplied machines), `name:address` at their first address, for the containers.
fn vm_hosts(env: &Environment) -> Vec<String> {
    env.machines
        .iter()
        .filter(|(_, m)| m.shape != MachineShape::Container)
        .filter_map(|(n, m)| m.addresses.first().map(|(_, a)| format!("{n}:{a}")))
        .collect()
}

/// The ports service `name` publishes: a machine's, or a tool's web UI (TCP).
fn ports_of(env: &Environment, name: &str) -> Vec<PublishedPort> {
    if let Some(mp) = env.machines.get(name) {
        return mp.ports.clone();
    }
    let tool = name.strip_prefix("isoloom-tool-").and_then(|t| env.spec().tools.get(t));
    match tool.map(|t| (t.port, t.publish)) {
        Some((Some(port), Some(publish))) => vec![PublishedPort {
            name: "web".into(),
            port,
            publish,
            protocol: Protocol::Tcp,
            fixed: true,
        }],
        _ => Vec::new(),
    }
}

/// A service with published ports: each at its fixed host port, on every address of the Docker
/// host (Vagrant forwards it to this machine's loopback), through Docker's bridge network
/// (macvlan can't publish), its default route staying on its lab networks.
fn publish(svc: &mut Service, ports: &[PublishedPort], published: &mut Vec<(u16, Protocol)>) {
    svc.ports = ports
        .iter()
        .map(|p| {
            published.push((p.publish, p.protocol));
            let proto = if p.protocol.is_tcp() { "" } else { "/udp" };
            format!("{}:{}{proto}", p.publish, p.port)
        })
        .collect();
    for a in svc.networks.values_mut().flatten() {
        a.gw_priority = Some(1);
    }
    svc.networks.insert(PUBLISH_NET.into(), None);
}

/// The networks the Docker host created, by name, after the publishing bridge when a container
/// publishes.
fn external_networks(env: &Environment, nets: &[String], publishing: bool) -> IndexMap<String, Network> {
    let mut out = IndexMap::new();
    if publishing {
        out.insert(PUBLISH_NET.to_string(), Network::default());
    }
    for net in nets {
        let n = Network {
            external: true,
            name: Some(docker_net(env, net)),
            ..Network::default()
        };
        out.insert(net.clone(), n);
    }
    out
}

fn contents(compose: &ComposeFile) -> Result<String, RenderError> {
    let yaml = crate::to_yaml(Target::Hybrid, compose)?;
    Ok(format!(
        "{}# The containers of a hybrid environment: run on the isoloom-docker VM, on the networks it\n# creates there (macvlan, shared with the VMs). Started by the Vagrantfile next to it.\n\n{yaml}",
        header("#")
    ))
}
