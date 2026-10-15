//! The pod template a Deployment or Job runs: its spec, placement and pod-wide settings.

use indexmap::IndexMap;
use serde::Serialize;

use super::{Container, Labels, Metadata, Selector, Volume};

/// A pod template: its labels and spec.
#[derive(Debug, Clone, Serialize)]
pub struct PodTemplate {
    /// Its labels.
    pub metadata: Metadata,
    /// The pod.
    pub spec: PodSpec,
}

/// A pod's spec.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    /// `Never` for a check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<&'static str>,
    /// The machine's name, as its own hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Its domain's label: the pod's full name is `<hostname>.<subdomain>.<namespace>.svc.<cluster
    /// domain>` (in its hosts file, so `hostname -f` answers it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    /// Where it may run (beside the owners of the volumes it shares).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affinity: Option<Affinity>,
    /// Its namespaced sysctls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_context: Option<PodSecurity>,
    /// What runs before it, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub init_containers: Vec<Container>,
    /// What runs.
    pub containers: Vec<Container>,
    /// The node labels it needs (its architecture).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub node_selector: Labels,
    /// `None` when it has its own resolver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_policy: Option<&'static str>,
    /// Its own resolver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_config: Option<DnsConfig>,
    /// Its volumes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<Volume>,
}

/// A pod's scheduling constraints.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Affinity {
    /// Pods it must run beside.
    pub pod_affinity: PodAffinity,
}

/// Pods a pod must run beside.
#[derive(Debug, Clone, Serialize)]
pub struct PodAffinity {
    /// Each term: a node running a pod the selector matches.
    #[serde(rename = "requiredDuringSchedulingIgnoredDuringExecution")]
    pub required: Vec<AffinityTerm>,
}

/// One pod-affinity term.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityTerm {
    /// The pods.
    pub label_selector: Selector,
    /// What "beside" means (the same node).
    pub topology_key: &'static str,
}

/// A pod's security settings: its sysctls.
#[derive(Debug, Clone, Serialize)]
pub struct PodSecurity {
    /// Kernel parameters, namespaced to the pod.
    pub sysctls: Vec<NameValue>,
}

/// A `name` and a `value` (a sysctl, a fixed environment variable).
#[derive(Debug, Clone, Serialize)]
pub struct NameValue {
    /// The name.
    pub name: String,
    /// The value.
    pub value: String,
}

/// A pod's own resolver.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DnsConfig {
    /// Its servers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nameservers: Vec<String>,
    /// Its search domains.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub searches: Vec<String>,
    /// Its resolver options (`ndots`, `timeout`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<DnsOption>,
}

/// A resolver option: `name`, and `value` when it takes one.
#[derive(Debug, Clone, Serialize)]
pub struct DnsOption {
    /// The option (`ndots`).
    pub name: String,
    /// Its value (`2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl DnsOption {
    /// An option as resolv.conf writes it: `name` or `name:value`.
    pub fn parse(option: &str) -> DnsOption {
        let (name, value) = option.split_once(':').map_or((option, None), |(n, v)| (n, Some(v.to_string())));
        DnsOption { name: name.to_string(), value }
    }
}
