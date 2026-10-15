//! Services and network policies: how pods answer, and who may reach them.

use serde::Serialize;

use super::{Labels, Selector};

/// A Service's spec.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceSpec {
    /// `LoadBalancer` when published; cluster-internal otherwise.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<&'static str>,
    /// `PreferDualStack` or `SingleStack` on IPv6 networks; the cluster's default otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ip_family_policy: Option<&'static str>,
    /// `IPv4` and `IPv6`, or `IPv6` alone, on IPv6 networks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ip_families: Vec<&'static str>,
    /// The pods behind it.
    pub(crate) selector: Labels,
    /// Its ports.
    pub(crate) ports: Vec<ServicePort>,
}

/// A Service's port.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServicePort {
    /// Unique within the Service.
    pub(crate) name: String,
    /// The port the Service answers on.
    pub(crate) port: u16,
    /// The pod's port.
    pub(crate) target_port: u16,
    /// `UDP`; TCP (the default) is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol: Option<&'static str>,
}

/// A `NetworkPolicy`'s spec.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkPolicySpec {
    /// The pods it applies to.
    pub(crate) pod_selector: Selector,
    /// The directions it restricts (by default, those it has rules for).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) policy_types: Vec<&'static str>,
    /// What may reach them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ingress: Vec<Rule>,
    /// What they may reach.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) egress: Vec<Rule>,
}

/// A `NetworkPolicy` rule: peers (`from` for ingress, `to` for egress), on ports.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Rule {
    /// Ingress peers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) from: Vec<Peer>,
    /// Egress peers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) to: Vec<Peer>,
    /// The ports (all when empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) ports: Vec<PolicyPort>,
}

/// A `NetworkPolicy` peer: pods of the namespace, every pod of other namespaces, or addresses.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Peer {
    /// Pods of the namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pod_selector: Option<Selector>,
    /// Every pod of the namespaces matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) namespace_selector: Option<Selector>,
    /// Addresses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ip_block: Option<IpBlock>,
}

impl Peer {
    /// The pods `selector` matches.
    pub(crate) fn pods(selector: Selector) -> Peer {
        Peer {
            pod_selector: Some(selector),
            ..Peer::default()
        }
    }
}

/// A block of addresses.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IpBlock {
    /// The block.
    pub(crate) cidr: String,
}

/// A `NetworkPolicy` port.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PolicyPort {
    /// The port.
    pub(crate) port: u16,
    /// `TCP` or `UDP`; left out for TCP where the default says so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol: Option<&'static str>,
}
