//! The networks of the Compose file, and a service's settings on one.

use std::net::Ipv4Addr;
use std::ops::Not;

use indexmap::IndexMap;
use serde::Serialize;

/// A network.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Network {
    /// Created outside this file (by `name`), not by Compose.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) external: bool,
    /// Its name on the Docker host, when not `<project>_<key>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    /// IPv4 off: an IPv6-only network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) enable_ipv4: Option<bool>,
    /// IPv6 on: a dual-stack or IPv6-only network.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) enable_ipv6: bool,
    /// Its address blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ipam: Option<Ipam>,
    /// No route out of the network: its containers can't reach the outside.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) internal: bool,
    /// The network's labels (Isoloom's own, to find what it created).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) labels: IndexMap<String, String>,
}

/// A network's addressing.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Ipam {
    /// The blocks, one per subnet.
    pub(crate) config: Vec<Block>,
}

/// One block of a network.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Block {
    /// The block, in CIDR notation.
    pub(crate) subnet: String,
    /// The address Docker takes on the network, when not the first one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gateway: Option<String>,
    /// Where Docker picks the addresses it hands out, away from the fixed ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ip_range: Option<String>,
}

/// A service's settings on one network.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Attachment {
    /// The fixed address on the network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ipv4_address: Option<String>,
    /// The fixed IPv6 address on an IPv6 network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ipv6_address: Option<String>,
    /// Other names the service answers to on the network.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) aliases: Vec<String>,
    /// The fixed MAC address of the interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mac_address: Option<String>,
    /// The interface's name in the container (`eth1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) interface_name: Option<String>,
    /// The order Compose attaches networks in (the highest first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) priority: Option<u32>,
    /// Which attachment gives the default route (the highest wins).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gw_priority: Option<u32>,
}

impl Attachment {
    /// At a fixed IPv4 address.
    pub(crate) fn at(address: Ipv4Addr) -> Attachment {
        Attachment {
            ipv4_address: Some(address.to_string()),
            ..Attachment::default()
        }
    }

    /// With a fixed MAC address (a trunk's end, which its machine finds its interface by).
    pub(crate) fn with_mac(mac: impl Into<String>) -> Attachment {
        Attachment {
            mac_address: Some(mac.into()),
            ..Attachment::default()
        }
    }
}
