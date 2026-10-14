//! Kubernetes: the object kinds of the core, apps, batch and networking APIs, then every field of
//! the definitions Isoloom writes, from the pinned OpenAPI document
//! (`coverage/kubernetes/openapi.txt`, see `scripts/refresh-kubernetes.py`).
//!
//! A definition gets rows of its own when it is an emitted kind, its spec, the pod spec or the
//! container, or when Isoloom writes into it. A deeper definition Isoloom doesn't write (a probe's
//! HTTP action, a node affinity) is the one row of the field that holds it: its fields would add
//! rows without adding decisions. The tests fail when Isoloom starts writing into a definition
//! that has no rows, so the depth follows the generator.

mod objects;
mod pod;
pub mod schema;
mod storage;

use crate::Format;
use crate::Support::{self, Emitted, Equivalent, InImage, Tooling};
pub use schema::{Field, Kind, Schema};

const OPENAPI: &str = include_str!("../../coverage/kubernetes/openapi.txt");

/// The pinned Kubernetes OpenAPI fields, as `scripts/refresh-kubernetes.py` wrote them.
pub fn openapi() -> Schema {
    Schema::parse(OPENAPI)
}

/// The definitions with rows of their own, in the table's order.
pub const TABLED: &[&str] = &[
    // The kinds Isoloom emits, and their specs.
    "Namespace",
    "NamespaceSpec",
    "Deployment",
    "DeploymentSpec",
    "DeploymentStrategy",
    "PodTemplateSpec",
    "Pod",
    "PodSpec",
    "Container",
    "ContainerPort",
    "EnvVar",
    "EnvVarSource",
    "SecretKeySelector",
    "ResourceRequirements",
    "Probe",
    "ExecAction",
    "HTTPGetAction",
    "SecurityContext",
    "PodSecurityContext",
    "Sysctl",
    "PodDNSConfig",
    "PodDNSConfigOption",
    "Affinity",
    "PodAffinity",
    "PodAffinityTerm",
    "LabelSelector",
    "Volume",
    "VolumeMount",
    "VolumeDevice",
    "PersistentVolumeClaimVolumeSource",
    "ConfigMapVolumeSource",
    "EmptyDirVolumeSource",
    "SecretVolumeSource",
    "KeyToPath",
    "PersistentVolumeClaim",
    "PersistentVolumeClaimSpec",
    "VolumeResourceRequirements",
    "Service",
    "ServiceSpec",
    "ServicePort",
    "NetworkPolicy",
    "NetworkPolicySpec",
    "NetworkPolicyIngressRule",
    "NetworkPolicyEgressRule",
    "NetworkPolicyPeer",
    "IPBlock",
    "NetworkPolicyPort",
    "Job",
    "JobSpec",
    "ConfigMap",
    "Secret",
    "ObjectMeta",
];

/// The section of the object kinds' rows.
pub const KINDS: &str = "Kinds";

/// The Kubernetes table: the kinds, then the tabled definitions' fields.
pub fn format() -> Format {
    let schema = openapi();
    let mut rows: Vec<(String, Support)> = schema
        .kinds
        .iter()
        .map(|k| (schema::key(KINDS, k.name), kind(k.name).unwrap_or(Support::Unclassified)))
        .collect();
    rows.extend(schema::rows(&schema, TABLED, classify));
    Format {
        name: "Kubernetes",
        file: ".isoloom/kubernetes/environment.yaml",
        source: "the Kubernetes v1.37.0 OpenAPI document (the version CI's kind cluster runs)",
        rows,
        collapse_not_portable: false,
    }
}

/// What Isoloom does with a field of a tabled definition.
fn classify(definition: &str, field: &str) -> Option<Support> {
    pod::classify(definition, field)
        .or_else(|| storage::classify(definition, field))
        .or_else(|| objects::classify(definition, field))
}

const ADMIN: Support = Tooling {
    note: "cluster administration: what the cluster runs on, not what an environment is",
};

/// What Isoloom does with an object kind.
fn kind(name: &str) -> Option<Support> {
    Some(match name {
        "Namespace" => Emitted {
            from: "(the environment's own namespace, from its name and instance)",
        },
        "Deployment" => Emitted {
            from: "machines (one pod each)",
        },
        "Service" => Emitted {
            from: "machines.*.services (its name for the others, and what it publishes)",
        },
        "NetworkPolicy" => Emitted { from: "networks and reach" },
        "PersistentVolumeClaim" => Emitted { from: "volumes" },
        "Job" | "Pod" => Emitted {
            from: "checks (a Job per position; `helm test` Pods in the chart)",
        },
        "ConfigMap" => Emitted {
            from: "(the project's scripts the pods run: `init:` jobs, checks)",
        },
        "Secret" => Emitted { from: "inputs (in the chart)" },
        "StatefulSet" | "ReplicaSet" | "ReplicationController" => Equivalent {
            via: "a Deployment of one replica, with its claims",
        },
        "CronJob" => InImage {
            note: "a scheduled task is the machine's own (cron in its image or its set-up)",
        },
        "Ingress" => Equivalent {
            via: "`publish`: a LoadBalancer Service on the published port",
        },
        "DaemonSet" => Tooling {
            note: "one pod per node: the cluster's agents, not an environment's machines",
        },
        "ServiceAccount" => Tooling {
            note: "an identity for the Kubernetes API: machines don't call it",
        },
        "Binding" | "ComponentStatus" | "ControllerRevision" | "Endpoints" | "Event" | "IPAddress" | "IngressClass" | "LimitRange" | "Node"
        | "PersistentVolume" | "PodTemplate" | "ResourceQuota" | "ServiceCIDR" => ADMIN,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tabled_definition_is_in_the_openapi() {
        let schema = openapi();
        for d in TABLED {
            assert!(schema.fields_of(d).next().is_some(), "{d} has no fields in the pinned OpenAPI");
        }
    }

    #[test]
    fn the_emitted_kinds_are_kinds_of_the_api() {
        let schema = openapi();
        for k in [
            "Namespace",
            "Deployment",
            "Service",
            "NetworkPolicy",
            "PersistentVolumeClaim",
            "Job",
            "Pod",
            "ConfigMap",
            "Secret",
        ] {
            let found = schema.kind(k).map(|k| k.definition);
            assert_eq!(found, Some(k), "{k}");
            assert!(TABLED.contains(&k), "{k} is emitted but has no rows");
        }
    }
}
