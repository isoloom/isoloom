//! The containers' Compose file: the Compose backend's, for the container machines alone,
//! adapted to run on the Docker host VM. Its networks are the macvlan networks the host creates
//! (external), the router is the router VM, the checks run from the VM side, and a container
//! reaches the VMs by name through `extra_hosts` (Docker's DNS only knows containers).

use indexmap::IndexMap;
use isoloom_compose::file::{ComposeFile, Network, Service};
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::backend::RenderError;
use isoloom_render::{GeneratedFile, header, router};
use isoloom_spec::model::{Protocol, Spec, Target};

use super::{as_hybrid, file, is_container};

/// The bridge network of the containers with published ports.
const PUBLISH_NET: &str = "isoloom-publish";

/// The containers' side of a hybrid environment.
pub(super) struct Containers {
    /// `.isoloom/hybrid/compose.yml`.
    pub file: GeneratedFile,
    /// The ports published on the host, in the order of the services: the Docker host forwards
    /// them.
    pub published: Vec<(u16, Protocol)>,
    /// The networks containers are on, in spec order: the Docker host creates them.
    pub networks: Vec<String>,
}

/// The Docker network a container network is on the host: named after the environment.
pub(super) fn docker_net(env: &Environment, net: &str) -> String {
    format!("isoloom-{}-{net}", env.name)
}

/// The Compose file of `env`'s containers.
pub(super) fn render(env: &Environment) -> Result<Containers, RenderError> {
    let alone = Environment::build(&containers_only(env.spec()), Target::Hybrid, env.instance);
    let mut compose = isoloom_compose::compose_file(Target::Docker, &alone).map_err(as_hybrid)?.file;
    compose.services.shift_remove(router::NAME);
    let names: Vec<String> = compose.services.keys().cloned().collect();
    let vm_hosts = vm_hosts(env);
    let mut published = Vec::new();
    for (name, svc) in &mut compose.services {
        if let Some(mp) = alone.machines.get(name).filter(|m| !m.ports.is_empty()) {
            publish(svc, mp, &mut published);
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
    Ok(Containers {
        file: file("compose.yml", contents(&compose)?),
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
        .filter(|(n, _)| !is_container(env, n))
        .filter_map(|(n, m)| m.addresses.first().map(|(_, a)| format!("{n}:{a}")))
        .collect()
}

/// A container with published ports: each at its fixed host port, on every address of the
/// Docker host (Vagrant forwards it to this machine's loopback), through Docker's bridge network
/// (macvlan can't publish), its default route staying on its lab networks.
fn publish(svc: &mut Service, mp: &MachinePlan, published: &mut Vec<(u16, Protocol)>) {
    svc.ports = mp
        .ports
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
    let yaml = isoloom_compose::to_yaml(Target::Hybrid, compose)?;
    Ok(format!(
        "{}# The containers of a hybrid environment: run on the isoloom-docker VM, on the networks it\n# creates there (macvlan, shared with the VMs). Started by the Vagrantfile next to it.\n\n{yaml}",
        header("#")
    ))
}
