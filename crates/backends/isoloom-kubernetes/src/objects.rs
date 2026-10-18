//! The Kubernetes objects Isoloom writes, as typed structs: the fields it uses, in one fixed
//! order, each left out when empty. Serialized with `serde_yaml_ng`, the same objects for the
//! plain manifests and the Helm chart: the fields the chart templates are typed ([`Image`],
//! [`Text`]), so its writer swaps them for Helm expressions without touching the text.

mod container;
mod networking;
mod pod;

pub(crate) use container::{
    Container, ContainerPort, ContainerSecurity, EmptyDir, EnvVar, Exec, HttpGet, KeyToPath, Limits, Probe, Resources, SecretKey, SecretSource, Volume,
    VolumeDevice, VolumeMount, VolumeSource,
};
pub(crate) use networking::{IpBlock, NetworkPolicySpec, Peer, PolicyPort, Rule, ServicePort, ServiceSpec};
pub(crate) use pod::{Affinity, AffinityTerm, DnsConfig, DnsOption, NameValue, PodAffinity, PodSecurity, PodSpec, PodTemplate};

use std::fmt::{self, Write as _};

use indexmap::IndexMap;
use serde::{Serialize, Serializer};

/// Labels and selectors: ordered, as written.
pub(crate) type Labels = IndexMap<String, String>;

/// The YAML tag a Helm expression is serialized under, for the chart writer to find (see
/// [`crate::chart`]). It never reaches a written file.
pub(crate) const HELM_TAG: &str = "HelmExpression";

/// A Helm expression (`index .Values.images "web"`), tagged for the chart writer.
fn helm<S: Serializer>(serializer: S, expression: &str) -> Result<S::Ok, S::Error> {
    serializer.serialize_newtype_variant("Text", 1, HELM_TAG, expression)
}

/// A string the chart may fill at install time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Text {
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
pub(crate) enum Image {
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

/// `s` as a Go template string literal: quoted, its quotes and backslashes escaped, and its
/// control characters as escapes (a Go string holds no raw line break). Every escape it writes
/// is JSON's too, so a JSON reader reads it back.
pub(crate) fn go_string(s: &str) -> String {
    GoString(s).to_string()
}

/// Text written as a Go string literal.
struct GoString<'a>(&'a str);

impl fmt::Display for GoString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        for c in self.0.chars() {
            match c {
                '"' | '\\' => write!(f, "\\{c}")?,
                '\n' => f.write_str("\\n")?,
                '\r' => f.write_str("\\r")?,
                '\t' => f.write_str("\\t")?,
                c if c.is_control() => write!(f, "\\u{:04x}", u32::from(c))?,
                c => f.write_char(c)?,
            }
        }
        f.write_char('"')
    }
}

/// One object of the manifests.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    /// Its API group and version.
    pub(crate) api_version: &'static str,
    /// Its kind.
    pub(crate) kind: &'static str,
    /// Its name, labels and annotations.
    pub(crate) metadata: Metadata,
    /// A Secret's type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) secret_type: Option<&'static str>,
    /// What the object is, for the kinds that have a spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) spec: Option<Spec>,
    /// A `ConfigMap`'s or Secret's entries.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) data: IndexMap<String, Text>,
    /// A `ConfigMap`'s entries, base64-encoded.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) binary_data: IndexMap<String, Text>,
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
    pub(crate) fn namespace(name: &str) -> Manifest {
        Manifest::new("v1", "Namespace", name, None)
    }

    /// A machine.
    pub(crate) fn deployment(name: &str, spec: DeploymentSpec) -> Manifest {
        Manifest::new("apps/v1", "Deployment", name, Some(Spec::Deployment(spec)))
    }

    /// A name (and ports) for pods.
    pub(crate) fn service(name: &str, spec: ServiceSpec) -> Manifest {
        Manifest::new("v1", "Service", name, Some(Spec::Service(spec)))
    }

    /// Who may talk to whom.
    pub(crate) fn network_policy(name: &str, spec: NetworkPolicySpec) -> Manifest {
        Manifest::new("networking.k8s.io/v1", "NetworkPolicy", name, Some(Spec::NetworkPolicy(spec)))
    }

    /// A volume's storage.
    pub(crate) fn claim(name: &str, spec: ClaimSpec) -> Manifest {
        Manifest::new("v1", "PersistentVolumeClaim", name, Some(Spec::Claim(spec)))
    }

    /// A run to completion (the checks).
    pub(crate) fn job(name: &str, spec: JobSpec) -> Manifest {
        Manifest::new("batch/v1", "Job", name, Some(Spec::Job(spec)))
    }

    /// A bare pod (the chart's `helm test` hooks).
    pub(crate) fn pod(metadata: Metadata, spec: PodSpec) -> Manifest {
        Manifest {
            metadata,
            ..Manifest::new("v1", "Pod", "", Some(Spec::Pod(spec)))
        }
    }

    /// Files for pods, base64-encoded (the chart fills them from its own files).
    pub(crate) fn config_map(name: &str, binary_data: IndexMap<String, Text>) -> Manifest {
        Manifest {
            binary_data,
            ..Manifest::new("v1", "ConfigMap", name, None)
        }
    }

    /// Text files for pods (a machine's `files` written in the spec).
    pub(crate) fn text_config_map(name: &str, data: IndexMap<String, Text>) -> Manifest {
        Manifest {
            data,
            ..Manifest::new("v1", "ConfigMap", name, None)
        }
    }

    /// Secret values, base64-encoded.
    pub(crate) fn secret(name: &str, data: IndexMap<String, Text>) -> Manifest {
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
pub(crate) enum Spec {
    /// A Deployment's.
    Deployment(DeploymentSpec),
    /// A Service's.
    Service(ServiceSpec),
    /// A `NetworkPolicy`'s.
    NetworkPolicy(NetworkPolicySpec),
    /// A `PersistentVolumeClaim`'s.
    Claim(ClaimSpec),
    /// A Job's.
    Job(JobSpec),
    /// A Pod's.
    Pod(PodSpec),
}

/// An object's name, labels and annotations.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Metadata {
    /// Its name (absent on a pod template).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) name: String,
    /// Its labels.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) labels: Labels,
    /// Its annotations.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) annotations: Labels,
}

impl Metadata {
    /// Just a name.
    pub(crate) fn named(name: &str) -> Metadata {
        Metadata {
            name: name.to_owned(),
            ..Metadata::default()
        }
    }

    /// Just labels (a pod template's).
    pub(crate) fn labelled(labels: Labels) -> Metadata {
        Metadata { labels, ..Metadata::default() }
    }
}

/// A Deployment's spec: one replica of a machine, replaced (not rolled) when it changes.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentSpec {
    /// How many pods.
    pub(crate) replicas: u32,
    /// How a change replaces the pod.
    pub(crate) strategy: Strategy,
    /// The pods it owns.
    pub(crate) selector: Selector,
    /// The pod.
    pub(crate) template: PodTemplate,
}

/// A Deployment's update strategy.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Strategy {
    /// `Recreate`: a machine is one pod at a time (its claim is `ReadWriteOnce`).
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
}

/// A label selector; empty selects every pod (`{}`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Selector {
    /// The labels a pod must carry.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) match_labels: Labels,
}

impl Selector {
    /// Pods carrying `key: value`.
    pub(crate) fn of(key: &str, value: &str) -> Selector {
        Selector {
            match_labels: Labels::from([(key.to_owned(), value.to_owned())]),
        }
    }
}

/// A `PersistentVolumeClaim`'s spec.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaimSpec {
    /// How nodes mount it.
    pub(crate) access_modes: Vec<&'static str>,
    /// `Filesystem` (mounted) or `Block` (a raw device), when Isoloom says which.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) volume_mode: Option<&'static str>,
    /// Its size.
    pub(crate) resources: ClaimResources,
}

/// What a claim asks for.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClaimResources {
    /// Its storage.
    pub(crate) requests: IndexMap<&'static str, String>,
}

/// A Job's spec: one pod, never retried.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSpec {
    /// Retries (none: a failed check is the result).
    pub(crate) backoff_limit: u32,
    /// The pod.
    pub(crate) template: PodTemplate,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_go_string_escapes_quotes_and_backslashes() {
        assert_eq!(go_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(go_string("a\nb\u{1}"), r#""a\nb\u0001""#);
    }

    proptest! {
        #[test]
        fn a_go_string_reads_back_as_the_text(text in ".*") {
            let quoted = go_string(&text);
            prop_assert!(!quoted.contains('\n'), "{quoted}");
            prop_assert_eq!(serde_json::from_str::<String>(&quoted).expect("a string literal"), text);
        }
    }

    #[test]
    fn literal_text_is_written_as_it_is() {
        assert_eq!(serde_yaml_ng::to_string(&Text::Literal("x".into())).unwrap(), "x\n");
    }
}
