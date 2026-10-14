//! Networks: address blocks, their Docker and traffic-control settings, VLANs, and the
//! reach between them.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A network: an address block, whether it reaches the internet, and what routes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// An IPv4 block inside 10.0.0.0/8, e.g. 10.20.0.0/24.
    pub cidr: String,
    /// Whether machines on it may reach the internet (default true).
    #[serde(default = "yes")]
    pub internet: bool,
    /// A machine of the environment that routes this network (an edge firewall): it takes the
    /// gateway address, and its own configuration decides what crosses it. Isoloom adds no
    /// router of its own here, so `reach` and `internet` on this network become expected
    /// behavior for the checks to verify.
    #[serde(default)]
    pub gateway: Option<String>,
    /// How the Docker target lays this network out, when it differs from the other targets.
    #[serde(default)]
    pub docker: Option<NetworkDocker>,
    /// The VLANs this LAN is split into, by id (1-4094), each with its own block inside the
    /// LAN's. Machines join one as `<lan>.vlan<id>` (`office.vlan10`); a `reach` rule naming
    /// the LAN covers all of its VLANs. Each VLAN becomes a network of its own,
    /// `<lan>-vlan<id>`; the LAN itself stays a network only when a machine joins it directly.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub vlans: IndexMap<u16, Vlan>,
    /// On a LAN split into `vlans`: a switch appliance of the environment (`docker.appliance:
    /// cisco-iol-l2` or `cisco-vios-l2`) that switches it, instead of Isoloom's own Linux switch.
    /// Its ports are derived, so the machine lists no `networks`: an access port per VLAN, then
    /// a trunk per machine on several of the LAN's VLANs. Docker targets only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<String>,
    /// Set by parsing on the networks a LAN's VLANs become: that LAN and the VLAN's id, so a
    /// target can carry them on one 802.1Q segment (a machine on several of them gets a trunk).
    #[serde(skip)]
    #[schemars(skip)]
    pub vlan: Option<(String, u16)>,
    /// Link impairment on traffic entering this network through Isoloom's router (a slow or
    /// lossy link): delay, jitter, loss, rate. Needs the router on the network (a `reach` rule
    /// touching it, no `gateway` machine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tc: Option<Tc>,
}

/// Link impairment (Linux netem on the router's interface into the network).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tc {
    /// One-way delay added, e.g. `50ms`, `1s`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    /// Random variation of the delay, e.g. `5ms` (needs `delay`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter: Option<String>,
    /// Packets dropped, in percent (`0.5` = 0.5%).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss: Option<f64>,
    /// Bandwidth cap, e.g. `10mbit`, `512kbit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,
}

impl Tc {
    /// The netem parameters, as `tc qdisc ... netem` takes them.
    pub fn netem(&self) -> String {
        let mut parts = Vec::new();
        if let Some(d) = &self.delay {
            parts.push(format!("delay {d}"));
            if let Some(j) = &self.jitter {
                parts.push(j.clone());
            }
        }
        if let Some(l) = self.loss {
            parts.push(format!("loss {l}%"));
        }
        if let Some(r) = &self.rate {
            parts.push(format!("rate {r}"));
        }
        parts.join(" ")
    }
}

/// A VLAN of a LAN (see [`Network::vlans`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vlan {
    /// Its IPv4 block, inside the LAN's (e.g. 10.10.10.0/24 in 10.10.0.0/16).
    pub cidr: String,
    /// Whether machines on it may reach the internet (default: as the LAN).
    #[serde(default)]
    pub internet: Option<bool>,
}

/// A network on the Docker target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkDocker {
    /// The block to use on Docker instead of `cidr` (same size, inside 10.0.0.0/8). Without
    /// it, a network outside 10.0.0.0/8 is moved into 10.0.0.0/8 automatically, since Docker's
    /// own pools, Docker Desktop and home networks use 172.16.0.0/12 and 192.168.0.0/16.
    pub cidr: String,
}

fn yes() -> bool {
    true
}

/// Traffic allowed from one network to another (one way: answers come back), on some ports or
/// all of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reach {
    /// The network the connections start from.
    pub from: String,
    /// The network they may reach.
    pub to: String,
    /// Empty = every port.
    #[serde(default)]
    pub ports: Vec<u16>,
}
