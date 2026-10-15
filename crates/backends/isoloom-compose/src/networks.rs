//! The environment's networks on Compose: each on its block with Docker at the host's address,
//! which of them are Docker's internal networks, and how each machine is attached and finds the
//! machines it shares no network with.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::{router, trunks};

use crate::appliances;
use crate::file::{Attachment, Block, Ipam, Network};
use crate::ipv6;
use crate::machine::Machine;

/// The networks: each on its block, Docker at the host's address. Without routing,
/// `internet: false` is Docker's internal network; then the trunks' links and the appliances'
/// management network.
pub(crate) fn declared(env: &Environment, ts: &[trunks::Trunk]) -> IndexMap<String, Network> {
    let mut out: IndexMap<String, Network> = env
        .networks
        .iter()
        .map(|(name, n)| {
            let subnet = Block {
                subnet: n.cidr.clone(),
                gateway: Some(n.host.to_string()),
                ip_range: None,
            };
            (name.clone(), ipv6::network(n, subnet, internal(env, name)))
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
        let subnet = Block {
            subnet: appliances::MGMT_CIDR.into(),
            gateway: None,
            ip_range: None,
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

/// Whether a network is Docker's internal network: offline, nothing routes, and no machine on
/// it publishes a port (Docker can't publish from internal networks). With a router or a
/// gateway, Docker's internal-network firewall would drop forwarded traffic, so offline
/// machines lose their default route instead (in their sidecar).
pub(crate) fn internal(env: &Environment, network: &str) -> bool {
    let offline = env.networks.get(network).is_some_and(|n| !n.internet);
    let publishes = env
        .machines
        .values()
        .any(|m| m.shape == MachineShape::Container && m.addresses.contains_key(network) && !m.ports.is_empty());
    offline && !router::routed(env.spec()) && !publishes
}

/// Docker's own address on `network`: where a machine with no other way out sends its traffic.
/// Every network of the spec has its plan, so the unspecified address only answers a name the
/// environment doesn't have.
pub(crate) fn docker_address(env: &Environment, network: &str) -> Ipv4Addr {
    env.networks.get(network).map_or(Ipv4Addr::UNSPECIFIED, |n| n.host)
}

/// A machine's networks, each at its address, with its aliases (resolved by Compose DNS).
pub(crate) fn attachments(env: &Environment, plan: &MachinePlan) -> IndexMap<String, Option<Attachment>> {
    let aliases: Vec<String> = plan.names.iter().skip(1).cloned().collect();
    plan.addresses
        .iter()
        .map(|(n, a)| {
            let at = Attachment {
                aliases: aliases.clone(),
                ..ipv6::attachment(env, plan, n, *a)
            };
            (n.clone(), Some(at))
        })
        .collect()
}

/// Names of machines that share no network with `m` (Compose DNS only resolves machines on
/// shared networks, and not across a network a trunk carries); they're reached through the
/// router. A LAN's switch has no address on the environment's networks: no name to resolve.
pub(crate) fn extra_hosts(env: &Environment, m: Machine<'_>) -> Vec<String> {
    let ts = trunks::trunks(env.spec());
    let shared = |other: &str, n: &str| m.spec.networks.contains_key(n) && !trunks::carried(&ts, m.name, n) && !trunks::carried(&ts, other, n);
    env.machines
        .iter()
        .filter(|(o, om)| o.as_str() != m.name && !om.addresses.is_empty() && !om.addresses.keys().any(|n| shared(o, n)))
        .flat_map(|(o, om)| {
            let a = env.address_for(m.name, o);
            om.names.iter().map(move |n| format!("{n}:{a}"))
        })
        .collect()
}
