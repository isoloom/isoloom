//! Services and network policies: how pods answer, and who may reach them.

use serde::Serialize;

use super::{Labels, Selector};

/// A Service's spec.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceSpec {
    /// `LoadBalancer` when published; cluster-internal otherwise.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    /// The pods behind it.
    pub selector: Labels,
    /// Its ports.
    pub ports: Vec<ServicePort>,
}

/// A Service's port.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicePort {
    /// Unique within the Service.
    pub name: String,
    /// The port the Service answers on.
    pub port: u16,
    /// The pod's port.
    pub target_port: u16,
    /// `UDP`; TCP (the default) is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<&'static str>,
}

/// A NetworkPolicy's spec.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicySpec {
    /// The pods it applies to.
    pub pod_selector: Selector,
    /// The directions it restricts (by default, those it has rules for).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub policy_types: Vec<&'static str>,
    /// What may reach them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ingress: Vec<Rule>,
    /// What they may reach.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub egress: Vec<Rule>,
}

/// A NetworkPolicy rule: peers (`from` for ingress, `to` for egress), on ports.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Rule {
    /// Ingress peers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub from: Vec<Peer>,
    /// Egress peers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<Peer>,
    /// The ports (all when empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PolicyPort>,
}

/// A NetworkPolicy peer: pods of the namespace, every pod of other namespaces, or addresses.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    /// Pods of the namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod_selector: Option<Selector>,
    /// Every pod of the namespaces matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_selector: Option<Selector>,
    /// Addresses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_block: Option<IpBlock>,
}

impl Peer {
    /// The pods `selector` matches.
    pub fn pods(selector: Selector) -> Peer {
        Peer {
            pod_selector: Some(selector),
            ..Peer::default()
        }
    }
}

/// A block of addresses.
#[derive(Debug, Clone, Serialize)]
pub struct IpBlock {
    /// The block.
    pub cidr: String,
}

/// A NetworkPolicy port.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyPort {
    /// The port.
    pub port: u16,
    /// `TCP` or `UDP`; left out for TCP where the default says so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<&'static str>,
}
