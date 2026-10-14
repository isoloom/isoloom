//! The Kubernetes objects Isoloom writes, as typed structs: the fields it uses, in one fixed
//! order, each left out when empty. Serialized with `serde_yaml_ng`, the same objects for the
//! plain manifests and the Helm chart: the fields the chart templates are typed ([`Image`],
//! [`Text`]), so its writer swaps them for Helm expressions without touching the text.

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
}

/// One container.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// Its name in the pod.
    pub name: String,
    /// What it runs.
    pub image: Image,
    /// `IfNotPresent` for a machine's image (a built one is only local).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<&'static str>,
    /// Its command, instead of the image's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// Its privileges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_context: Option<ContainerSecurity>,
    /// The ports it serves.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<ContainerPort>,
    /// When it is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness_probe: Option<Probe>,
    /// Its environment.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,
    /// Its limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    /// Where it sees the pod's volumes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub volume_mounts: Vec<VolumeMount>,
}

impl Container {
    /// A container running `image`, nothing else set.
    pub fn new(name: impl Into<String>, image: Image) -> Container {
        Container {
            name: name.into(),
            image,
            image_pull_policy: None,
            command: Vec::new(),
            security_context: None,
            ports: Vec::new(),
            readiness_probe: None,
            env: Vec::new(),
            resources: None,
            volume_mounts: Vec::new(),
        }
    }
}

/// A container's privileges.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSecurity {
    /// Every capability, every device.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub privileged: bool,
    /// A read-only image.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub read_only_root_filesystem: bool,
    /// The user it runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_as_user: Option<u32>,
    /// The group it runs as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_as_group: Option<u32>,
}

/// A port a container serves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    /// The port.
    pub container_port: u16,
    /// `UDP`; TCP (the default) is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<&'static str>,
}

/// A readiness probe: a command, retried.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// The command.
    pub exec: Exec,
    /// Seconds between tries.
    pub period_seconds: u32,
    /// Tries before the pod is unready.
    pub failure_threshold: u32,
}

/// A command run in the container.
#[derive(Debug, Clone, Serialize)]
pub struct Exec {
    /// The command, exec form.
    pub command: Vec<String>,
}

/// An environment variable: fixed, or read from a Secret.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EnvVar {
    /// A fixed value.
    Value(NameValue),
    /// A key of a Secret.
    Secret {
        /// The variable.
        name: String,
        /// Where its value comes from.
        #[serde(rename = "valueFrom")]
        value_from: SecretSource,
    },
}

/// Where a variable's value comes from: a Secret's key.
#[derive(Debug, Clone, Serialize)]
pub struct SecretSource {
    /// The key.
    #[serde(rename = "secretKeyRef")]
    pub secret_key_ref: SecretKey,
}

/// A Secret's key, optional (unset is empty).
#[derive(Debug, Clone, Serialize)]
pub struct SecretKey {
    /// The Secret.
    pub name: String,
    /// The key.
    pub key: String,
    /// Whether the pod starts without it.
    pub optional: bool,
}

/// A container's limits.
#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    /// CPU and memory at most.
    pub limits: Limits,
}

/// CPU and memory limits.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Limits {
    /// CPUs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    /// Memory (`512Mi`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

/// Where a container sees a volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMount {
    /// The pod's volume.
    pub name: String,
    /// Where.
    pub mount_path: String,
    /// Whether it can't write to it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

impl VolumeMount {
    /// Volume `name` at `path`, writable.
    pub fn at(name: impl Into<String>, path: impl Into<String>) -> VolumeMount {
        VolumeMount {
            name: name.into(),
            mount_path: path.into(),
            read_only: false,
        }
    }

    /// Volume `name` at `path`, read-only.
    pub fn read_only(name: impl Into<String>, path: impl Into<String>) -> VolumeMount {
        VolumeMount {
            read_only: true,
            ..VolumeMount::at(name, path)
        }
    }
}

/// A pod's volume.
#[derive(Debug, Clone, Serialize)]
pub struct Volume {
    /// Its name in the pod.
    pub name: String,
    /// What it is.
    #[serde(flatten)]
    pub source: VolumeSource,
}

/// What a pod's volume is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeSource {
    /// A claim.
    PersistentVolumeClaim {
        /// The claim's name.
        #[serde(rename = "claimName")]
        claim_name: String,
    },
    /// A ConfigMap's files.
    ConfigMap {
        /// The ConfigMap.
        name: String,
        /// Their mode (executable).
        #[serde(rename = "defaultMode")]
        default_mode: u32,
    },
    /// Scratch space, on disk or in memory.
    EmptyDir(EmptyDir),
}

/// Scratch space; empty is on the node's disk (`{}`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyDir {
    /// `Memory`: a tmpfs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medium: Option<&'static str>,
    /// Its size at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_limit: Option<String>,
}

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
