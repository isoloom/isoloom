//! The objects around the pods: their metadata, the Deployment and Job that run them, the
//! Services and NetworkPolicies of the networks, and the ConfigMaps and Secrets they read.

use crate::Support::{self, Emitted, Equivalent, Tooling};

const OBJECT: Support = Emitted {
    from: "(every object Isoloom writes)",
};
const SERVER: Support = Tooling {
    note: "written by the cluster, not by a manifest",
};
const ROLLOUT: Support = Tooling {
    note: "how Kubernetes rolls out and retries: a machine is one pod, replaced when it changes",
};
const SERVICE_DETAIL: Support = Tooling {
    note: "how the cluster routes a Service: machines reach each other by name and port",
};
const POLICY: Support = Emitted {
    from: "networks and reach (deny by default; a network's machines reach each other)",
};
const IMMUTABLE: Support = Tooling {
    note: "an API safeguard: Isoloom replaces the object when the spec changes",
};

/// What Isoloom does with a field of the objects' definitions.
pub(super) fn classify(definition: &str, field: &str) -> Option<Support> {
    match definition {
        "Namespace" if field == "spec" => Some(Tooling {
            note: "a namespace's finalizers: how the API deletes it",
        }),
        "Namespace" | "Deployment" | "Pod" | "Service" | "NetworkPolicy" | "PersistentVolumeClaim" | "Job" => object(field),
        "ConfigMap" | "Secret" => data(definition, field),
        "ObjectMeta" | "NamespaceSpec" | "LabelSelector" | "PodTemplateSpec" => metadata(definition, field),
        "DeploymentSpec" | "DeploymentStrategy" | "JobSpec" => workload(definition, field),
        "ServiceSpec" | "ServicePort" => service(definition, field),
        "NetworkPolicySpec" | "NetworkPolicyIngressRule" | "NetworkPolicyEgressRule" | "NetworkPolicyPeer" | "IPBlock" | "NetworkPolicyPort" => {
            policy(definition, field)
        }
        _ => None,
    }
}

fn object(field: &str) -> Option<Support> {
    Some(match field {
        "apiVersion" | "kind" | "metadata" | "spec" => OBJECT,
        _ => return None,
    })
}

fn data(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        (_, "apiVersion" | "kind" | "metadata") => OBJECT,
        ("ConfigMap", "binaryData") => Emitted {
            from: "(the project's scripts, read from the chart's files)",
        },
        ("ConfigMap", "data") => Emitted {
            from: "machines.*.files[].content (the project's scripts are `binaryData`, byte for byte)",
        },
        ("Secret", "data") => Emitted { from: "inputs (in the chart)" },
        ("Secret", "type") => Emitted { from: "(Opaque)" },
        ("Secret", "stringData") => Equivalent { via: "`data`" },
        (_, "immutable") => IMMUTABLE,
        _ => return None,
    })
}

fn metadata(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("ObjectMeta", "name") => OBJECT,
        ("ObjectMeta", "labels") | ("LabelSelector", "matchLabels") => Emitted {
            from: "machines and networks (the labels the selectors and policies match)",
        },
        ("ObjectMeta", "annotations") => Emitted {
            from: "checks (`helm test` hooks, in the chart)",
        },
        ("ObjectMeta", "namespace") => Equivalent {
            via: "the kustomization's `namespace` and the Helm release's",
        },
        ("ObjectMeta", "finalizers" | "generateName" | "ownerReferences") | ("NamespaceSpec", "finalizers") => Tooling {
            note: "how the API names and deletes objects: Isoloom names each one",
        },
        ("ObjectMeta", _) => SERVER,
        ("LabelSelector", "matchExpressions") => Equivalent { via: "`matchLabels`" },
        ("PodTemplateSpec", "metadata" | "spec") => Emitted { from: "machines" },
        _ => return None,
    })
}

fn workload(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("DeploymentSpec", "replicas") => Emitted {
            from: "(one: a machine is one pod)",
        },
        ("DeploymentSpec", "selector" | "template") => Emitted { from: "machines" },
        ("DeploymentSpec", "strategy") | ("DeploymentStrategy", "type") => Emitted {
            from: "(Recreate: its claims are mounted by one pod at a time)",
        },
        ("JobSpec", "template") => Emitted {
            from: "checks (a Job per position)",
        },
        ("JobSpec", "backoffLimit") => Emitted {
            from: "checks (run once: a failure is a result)",
        },
        ("DeploymentSpec" | "DeploymentStrategy" | "JobSpec", _) => ROLLOUT,
        _ => return None,
    })
}

fn service(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("ServiceSpec", "ports" | "selector") | ("ServicePort", "name" | "port" | "targetPort" | "protocol") => Emitted {
            from: "machines.*.ports (and their `publish`)",
        },
        ("ServiceSpec", "type") => Emitted {
            from: "machines.*.ports[].publish (a LoadBalancer Service)",
        },
        ("ServiceSpec", "ipFamilies" | "ipFamilyPolicy") => Emitted {
            from: "networks.*.ipv6 (PreferDualStack; SingleStack IPv6 on IPv6-only networks)",
        },
        ("ServiceSpec", "externalIPs" | "loadBalancerIP") | ("ServicePort", "nodePort") => Equivalent {
            via: "`publish`: a LoadBalancer Service on the published port",
        },
        ("ServiceSpec", "loadBalancerSourceRanges") => Equivalent {
            via: "the published port's NetworkPolicy",
        },
        ("ServiceSpec" | "ServicePort", _) => SERVICE_DETAIL,
        _ => return None,
    })
}

fn policy(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("NetworkPolicySpec" | "NetworkPolicyIngressRule" | "NetworkPolicyEgressRule" | "NetworkPolicyPeer", _) => POLICY,
        ("IPBlock", "cidr") => Emitted {
            from: "machines.*.ports[].publish and offline networks (anyone in, or nothing out)",
        },
        ("IPBlock", "except") => Tooling {
            note: "carves addresses out of a range: Isoloom's rules name whole networks",
        },
        ("NetworkPolicyPort", "port" | "protocol") => Emitted {
            from: "reach[].ports and machines.*.ports[].publish",
        },
        ("NetworkPolicyPort", "endPort") => Equivalent {
            via: "`port`, one rule per port",
        },
        _ => return None,
    })
}
