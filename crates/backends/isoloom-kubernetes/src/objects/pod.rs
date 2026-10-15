//! The pod template a Deployment or Job runs: its spec, placement and pod-wide settings.

use indexmap::IndexMap;
use serde::Serialize;

use super::{Container, Labels, Metadata, Selector, Volume};

/// A pod template: its labels and spec.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PodTemplate {
    /// Its labels.
    pub(crate) metadata: Metadata,
    /// The pod.
    pub(crate) spec: PodSpec,
}

/// A pod's spec.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PodSpec {
    /// `Never` for a check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) restart_policy: Option<&'static str>,
    /// The machine's name, as its own hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hostname: Option<String>,
    /// Its domain's label: the pod's full name is `<hostname>.<subdomain>.<namespace>.svc.<cluster
    /// domain>` (in its hosts file, so `hostname -f` answers it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subdomain: Option<String>,
    /// Where it may run (beside the owners of the volumes it shares).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) affinity: Option<Affinity>,
    /// Its namespaced sysctls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) security_context: Option<PodSecurity>,
    /// What runs before it, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) init_containers: Vec<Container>,
    /// What runs.
    pub(crate) containers: Vec<Container>,
    /// The node labels it needs (its architecture).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) node_selector: Labels,
    /// `None` when it has its own resolver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dns_policy: Option<&'static str>,
    /// Its own resolver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dns_config: Option<DnsConfig>,
    /// Its volumes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) volumes: Vec<Volume>,
}

/// A pod's scheduling constraints.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Affinity {
    /// Pods it must run beside.
    pub(crate) pod_affinity: PodAffinity,
}

/// Pods a pod must run beside.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PodAffinity {
    /// Each term: a node running a pod the selector matches.
    #[serde(rename = "requiredDuringSchedulingIgnoredDuringExecution")]
    pub(crate) required: Vec<AffinityTerm>,
}

/// One pod-affinity term.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AffinityTerm {
    /// The pods.
    pub(crate) label_selector: Selector,
    /// What "beside" means (the same node).
    pub(crate) topology_key: &'static str,
}

/// A pod's security settings: its sysctls.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PodSecurity {
    /// Kernel parameters, namespaced to the pod.
    pub(crate) sysctls: Vec<NameValue>,
}

/// A `name` and a `value` (a sysctl, a fixed environment variable).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct NameValue {
    /// The name.
    pub(crate) name: String,
    /// The value.
    pub(crate) value: String,
}

/// A pod's own resolver.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct DnsConfig {
    /// Its servers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) nameservers: Vec<String>,
    /// Its search domains.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) searches: Vec<String>,
    /// Its resolver options (`ndots`, `timeout`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) options: Vec<DnsOption>,
}

/// A resolver option: `name`, and `value` when it takes one.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DnsOption {
    /// The option (`ndots`).
    pub(crate) name: String,
    /// Its value (`2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
}

impl From<&str> for DnsOption {
    /// An option as `resolv.conf` writes it: `name` or `name:value`.
    fn from(option: &str) -> DnsOption {
        let (name, value) = option.split_once(':').map_or((option, None), |(n, v)| (n, Some(v.to_owned())));
        DnsOption { name: name.to_owned(), value }
    }
}
