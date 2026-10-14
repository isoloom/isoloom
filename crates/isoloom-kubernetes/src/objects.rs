//! The Kubernetes objects Isoloom writes, as typed structs: the fields it uses, in one fixed
//! order, each left out when empty. Serialized with `serde_yaml_ng`, the same objects for the
//! plain manifests and the Helm chart: the fields the chart templates are typed ([`Image`],
//! [`Text`]), so its writer swaps them for Helm expressions without touching the text.

mod container;
mod networking;
mod pod;

pub use container::{
    Container, ContainerPort, ContainerSecurity, EmptyDir, EnvVar, Exec, Limits, Probe, Resources, SecretKey, SecretSource, Volume, VolumeMount, VolumeSource,
};
pub use networking::{IpBlock, NetworkPolicySpec, Peer, PolicyPort, Rule, ServicePort, ServiceSpec};
pub use pod::{Affinity, AffinityTerm, DnsConfig, NameValue, PodAffinity, PodSecurity, PodSpec, PodTemplate};

use indexmap::IndexMap;
use serde::{Serialize, Serializer};

/// Labels and selectors: ordered, as written.
pub type Labels = IndexMap<String, String>;

/// The YAML tag a Helm expression is serialized under, for the chart writer to find (see
/// [`crate::chart`]). It never reaches a written file.
pub const HELM_TAG: &str = "HelmExpression";

/// A Helm expression (`index .Values.images "web"`), tagged for the chart writer.
fn helm<S: Serializer>(serializer: S, expression: &str) -> Result<S::Ok, S::Error> {
    serializer.serialize_newtype_variant("Text", 1, HELM_TAG, expression)
}

/// A string the chart may fill at install time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    /// Written as it is.
    Literal(String),
    /// A Helm expression, without its braces: the chart writer wraps it in `{{ }}`.
    Helm(String),
}

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Text::Literal(s) => serializer.serialize_str(s),
            Text::Helm(e) => helm(serializer, e),
        }
    }
}

/// A container's image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// One of Isoloom's own (the probe, the waits, the check runner).
    Tool(String),
    /// The machine's image: a value of its own in the chart (`images.<machine>`).
    Machine {
        /// The machine it is the image of.
        machine: String,
        /// The reference the manifests pull (through the mirrors).
        reference: String,
    },
    /// The chart's value for the machine's image.
    Value {
        /// The machine.
        machine: String,
    },
}

impl Serialize for Image {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Image::Tool(r) | Image::Machine { reference: r, .. } => serializer.serialize_str(r),
            Image::Value { machine } => helm(serializer, &format!("index .Values.images {}", go_string(machine))),
        }
    }
}

/// `s` as a Go template string literal (a JSON string is one).
pub fn go_string(s: &str) -> String {
    let escaped: String = s
        .chars()
        .flat_map(|c| match c {
            '"' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect();
    format!("\"{escaped}\"")
}

/// One object of the manifests.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Its API group and version.
    pub api_version: &'static str,
    /// Its kind.
    pub kind: &'static str,
    /// Its name, labels and annotations.
    pub metadata: Metadata,
    /// A Secret's type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub secret_type: Option<&'static str>,
    /// What the object is, for the kinds that have a spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<Spec>,
    /// A ConfigMap's or Secret's entries.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub data: IndexMap<String, Text>,
    /// A ConfigMap's entries, base64-encoded.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub binary_data: IndexMap<String, Text>,
}

impl Manifest {
    fn new(api_version: &'static str, kind: &'static str, name: &str, spec: Option<Spec>) -> Manifest {
        Manifest {
            api_version,
            kind,
            metadata: Metadata::named(name),
            secret_type: None,
            spec,
            data: IndexMap::new(),
            binary_data: IndexMap::new(),
        }
    }

    /// The environment's namespace.
    pub fn namespace(name: &str) -> Manifest {
        Manifest::new("v1", "Namespace", name, None)
    }

    /// A machine.
    pub fn deployment(name: &str, spec: DeploymentSpec) -> Manifest {
        Manifest::new("apps/v1", "Deployment", name, Some(Spec::Deployment(spec)))
    }

    /// A name (and ports) for pods.
    pub fn service(name: &str, spec: ServiceSpec) -> Manifest {
        Manifest::new("v1", "Service", name, Some(Spec::Service(spec)))
    }

    /// Who may talk to whom.
    pub fn network_policy(name: &str, spec: NetworkPolicySpec) -> Manifest {
        Manifest::new("networking.k8s.io/v1", "NetworkPolicy", name, Some(Spec::NetworkPolicy(spec)))
    }

    /// A volume's storage.
    pub fn claim(name: &str, spec: ClaimSpec) -> Manifest {
        Manifest::new("v1", "PersistentVolumeClaim", name, Some(Spec::Claim(spec)))
    }

    /// A run to completion (the checks).
    pub fn job(name: &str, spec: JobSpec) -> Manifest {
        Manifest::new("batch/v1", "Job", name, Some(Spec::Job(spec)))
    }

    /// A bare pod (the chart's `helm test` hooks).
    pub fn pod(metadata: Metadata, spec: PodSpec) -> Manifest {
        Manifest {
            metadata,
            ..Manifest::new("v1", "Pod", "", Some(Spec::Pod(spec)))
        }
    }

    /// Files for pods, base64-encoded (the chart fills them from its own files).
    pub fn config_map(name: &str, binary_data: IndexMap<String, Text>) -> Manifest {
        Manifest {
            binary_data,
            ..Manifest::new("v1", "ConfigMap", name, None)
        }
    }

    /// Secret values, base64-encoded.
    pub fn secret(name: &str, data: IndexMap<String, Text>) -> Manifest {
        Manifest {
            secret_type: Some("Opaque"),
            data,
            ..Manifest::new("v1", "Secret", name, None)
        }
    }
}

/// An object's spec, by kind.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Spec {
    /// A Deployment's.
    Deployment(DeploymentSpec),
    /// A Service's.
    Service(ServiceSpec),
    /// A NetworkPolicy's.
    NetworkPolicy(NetworkPolicySpec),
    /// A PersistentVolumeClaim's.
    Claim(ClaimSpec),
    /// A Job's.
    Job(JobSpec),
    /// A Pod's.
    Pod(PodSpec),
}

/// An object's name, labels and annotations.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metadata {
    /// Its name (absent on a pod template).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Its labels.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub labels: Labels,
    /// Its annotations.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub annotations: Labels,
}

impl Metadata {
    /// Just a name.
    pub fn named(name: &str) -> Metadata {
        Metadata {
            name: name.to_string(),
            ..Metadata::default()
        }
    }

    /// Just labels (a pod template's).
    pub fn labelled(labels: Labels) -> Metadata {
        Metadata { labels, ..Metadata::default() }
    }
}

/// A Deployment's spec: one replica of a machine, replaced (not rolled) when it changes.
#[derive(Debug, Clone, Serialize)]
pub struct DeploymentSpec {
    /// How many pods.
    pub replicas: u32,
    /// How a change replaces the pod.
    pub strategy: Strategy,
    /// The pods it owns.
    pub selector: Selector,
    /// The pod.
    pub template: PodTemplate,
}

/// A Deployment's update strategy.
#[derive(Debug, Clone, Serialize)]
pub struct Strategy {
    /// `Recreate`: a machine is one pod at a time (its claim is ReadWriteOnce).
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// A label selector; empty selects every pod (`{}`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selector {
    /// The labels a pod must carry.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub match_labels: Labels,
}

impl Selector {
    /// Pods carrying `key: value`.
    pub fn of(key: &str, value: &str) -> Selector {
        Selector {
            match_labels: Labels::from([(key.to_string(), value.to_string())]),
        }
    }
}

/// A PersistentVolumeClaim's spec.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimSpec {
    /// How nodes mount it.
    pub access_modes: Vec<&'static str>,
    /// Its size.
    pub resources: ClaimResources,
}

/// What a claim asks for.
#[derive(Debug, Clone, Serialize)]
pub struct ClaimResources {
    /// Its storage.
    pub requests: IndexMap<&'static str, String>,
}

/// A Job's spec: one pod, never retried.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    /// Retries (none: a failed check is the result).
    pub backoff_limit: u32,
    /// The pod.
    pub template: PodTemplate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_go_string_escapes_quotes_and_backslashes() {
        assert_eq!(go_string(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn literal_text_is_written_as_it_is() {
        assert_eq!(serde_yaml_ng::to_string(&Text::Literal("x".into())).unwrap(), "x\n");
    }
}
