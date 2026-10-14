//! The pod and its containers: what a machine is on Kubernetes.

use crate::Support::{self, Emitted, Equivalent, InImage, NotPortable, Partial, Planned, Tooling};

/// Kubernetes' own handling of pods: where they run, how they are restarted or stopped.
const SCHEDULING: Support = Tooling {
    note: "Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves",
};
/// Container isolation: Isoloom's rule leaves these out of the machine features.
const CONTAINER: Support = Tooling {
    note: "container mechanics (users, profiles, namespaces): not a machine feature",
};
const API: Support = Tooling {
    note: "access to the Kubernetes API: machines don't call it",
};
const HOST: Support = NotPortable {
    why: "shares the node's own namespaces: cloud VMs have no host to share",
};
const DEVICES: Support = Planned {
    note: "device passthrough (GPUs), where the cluster offers it",
};
const WINDOWS: Support = Planned {
    note: "Windows machines in Windows containers (Windows makes an environment VM-only today)",
};
const HEALTHCHECK: Support = Planned {
    note: "with a custom healthcheck command (the probe checks the services' ports today)",
};
const ATTACH: Support = Tooling {
    note: "a terminal for whoever attaches: `isoloom connect` opens its own",
};

/// What Isoloom does with a field of the pod's definitions.
pub(super) fn classify(definition: &str, field: &str) -> Option<Support> {
    match definition {
        "PodSpec" => pod_spec(field),
        "Container" => container(field),
        "ContainerPort" | "Probe" | "ExecAction" | "ResourceRequirements" => container_parts(definition, field),
        "EnvVar" | "EnvVarSource" | "SecretKeySelector" => environment(definition, field),
        "SecurityContext" => security(field),
        "PodSecurityContext" | "Sysctl" | "PodDNSConfig" | "PodDNSConfigOption" => pod_settings(definition, field),
        "Affinity" | "PodAffinity" | "PodAffinityTerm" => affinity(definition, field),
        _ => None,
    }
}

fn pod_spec(field: &str) -> Option<Support> {
    if let Some(s) = pod_names(field) {
        return Some(s);
    }
    Some(match field {
        "containers" => Emitted {
            from: "machines (the machine's container, and one for its `init:` jobs)",
        },
        "initContainers" => Emitted {
            from: "machines.*.depends_on (waits for the dependencies' services), and Isoloom's probe and volume seeds",
        },
        "hostname" => Emitted { from: "(the machine's name)" },
        "affinity" => Emitted {
            from: "volumes (a machine sharing a volume runs on its owner's node)",
        },
        "dnsConfig" | "dnsPolicy" => Emitted { from: "machines.*.dns" },
        "nodeSelector" => Emitted {
            from: "machines.*.arch (a node of the machine's architecture)",
        },
        "restartPolicy" => Emitted { from: "checks (run once)" },
        "securityContext" => Emitted { from: "machines.*.sysctls" },
        "volumes" => Emitted {
            from: "volumes, machines.*.tmpfs and machines.*.shm_size",
        },
        "os" => WINDOWS,
        "resourceClaims" => DEVICES,
        "resources" => Equivalent {
            via: "each container's `resources`",
        },
        "hostnameOverride" => Equivalent {
            via: "`hostname`, the machine's name",
        },
        "hostIPC" | "hostNetwork" | "hostPID" => HOST,
        "automountServiceAccountToken" | "serviceAccount" | "serviceAccountName" | "enableServiceLinks" => API,
        "hostUsers" | "runtimeClassName" | "shareProcessNamespace" | "overhead" => CONTAINER,
        "imagePullSecrets" => Tooling {
            note: "credentials of a private registry: the cluster's setup",
        },
        "ephemeralContainers" => Tooling {
            note: "debugging containers added to a running pod",
        },
        "activeDeadlineSeconds"
        | "evictionResponders"
        | "nodeName"
        | "preemptionPolicy"
        | "priority"
        | "priorityClassName"
        | "readinessGates"
        | "schedulerName"
        | "schedulingGates"
        | "schedulingGroup"
        | "terminationGracePeriodSeconds"
        | "tolerations"
        | "topologySpreadConstraints" => SCHEDULING,
        _ => return None,
    })
}

/// How the pod is named: its full name, the other names it answers to.
fn pod_names(field: &str) -> Option<Support> {
    Some(match field {
        "hostAliases" => NotPortable {
            why: "it points names at fixed addresses, and a pod has none: the spec's `aliases` name a machine, which Kubernetes reaches only by its Service's name (no dots), so Kubernetes refuses `aliases`",
        },
        "subdomain" => Partial {
            from: "machines.*.dns.domain, as one label (`corp.local` is `corp-local`)",
            gap: "the pod's full name is `<machine>.<subdomain>.<namespace>.svc.<cluster domain>`: Kubernetes names pods under the cluster's domain, not `dns.domain`",
        },
        "setHostnameAsFQDN" => Equivalent {
            via: "`subdomain`: the pod's full name is in its hosts file (`hostname -f`), and its hostname stays the short name, as on every other target",
        },
        _ => return None,
    })
}

fn container(field: &str) -> Option<Support> {
    Some(match field {
        "name" => Emitted { from: "(the machine's name)" },
        "image" => Emitted {
            from: "machines.*.docker.image (or the image the project builds; a value of its own in the chart)",
        },
        "imagePullPolicy" => Emitted {
            from: "(IfNotPresent: an image built on the node is used as it is)",
        },
        "command" => Emitted {
            from: "machines.*.docker.idle and machines.*.docker.init (and Isoloom's own containers)",
        },
        "env" => Emitted {
            from: "machines.*.environment and machines.*.inputs",
        },
        "ports" => Emitted { from: "machines.*.services" },
        "readinessProbe" => Partial {
            from: "machines.*.services (ready when its services' ports answer; dormant ones aren't waited for)",
            gap: "a custom healthcheck command",
        },
        "resources" => Emitted { from: "machines.*.resources" },
        "securityContext" => Emitted {
            from: "machines.*.privileged and machines.*.read_only",
        },
        "volumeMounts" => Emitted {
            from: "machines.*.volumes, machines.*.tmpfs and machines.*.shm_size",
        },
        "args" => Planned {
            note: "a machine's own `command` (the image's default runs today)",
        },
        "workingDir" => InImage { note: "the image's WORKDIR" },
        "envFrom" => Equivalent {
            via: "`env`, one variable at a time",
        },
        "lifecycle" => Equivalent {
            via: "machines.*.docker.init: a container beside the machine's, run once it answers",
        },
        "startupProbe" => Equivalent {
            via: "`readinessProbe`, which waits as long as the machine takes to start",
        },
        "livenessProbe" => Tooling {
            note: "restarts a container Kubernetes finds unhealthy: the cluster's self-healing, not the machine's behavior",
        },
        "volumeDevices" => Planned {
            note: "raw data disks, with persistent volumes and data disks",
        },
        "stdin" | "stdinOnce" | "tty" => ATTACH,
        "resizePolicy" | "restartPolicy" | "restartPolicyRules" | "terminationMessagePath" | "terminationMessagePolicy" => SCHEDULING,
        _ => return None,
    })
}

fn container_parts(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("ContainerPort", "containerPort") => Emitted {
            from: "machines.*.services[].port",
        },
        ("ContainerPort", "protocol") => Emitted {
            from: "machines.*.services[].protocol",
        },
        ("ContainerPort", "name") => Tooling {
            note: "a name for the port within Kubernetes: Isoloom refers to ports by number",
        },
        ("ContainerPort", "hostIP" | "hostPort") => Equivalent {
            via: "`publish`: a LoadBalancer Service on the published port",
        },
        ("Probe", "exec") | ("ExecAction", "command") => Emitted {
            from: "machines.*.services (Isoloom's port probe, the same on every target)",
        },
        ("Probe", "periodSeconds" | "failureThreshold") => Emitted {
            from: "(every 5 seconds, for up to 5 minutes)",
        },
        ("Probe", "tcpSocket") => Equivalent {
            via: "`exec`: Isoloom's port probe checks every service port, TCP and UDP",
        },
        ("Probe", "httpGet" | "grpc" | "initialDelaySeconds" | "successThreshold" | "timeoutSeconds") => HEALTHCHECK,
        ("Probe", "terminationGracePeriodSeconds") => SCHEDULING,
        ("ResourceRequirements", "limits") => Emitted {
            from: "machines.*.resources (cpus, memory_mb)",
        },
        ("ResourceRequirements", "requests") => Tooling {
            note: "what the scheduler reserves on a node: the limits are the machine's size",
        },
        ("ResourceRequirements", "claims") => DEVICES,
        _ => return None,
    })
}

fn environment(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("EnvVar", "name" | "value") => Emitted {
            from: "machines.*.environment (and Isoloom's own variables)",
        },
        ("EnvVar", "valueFrom") | ("EnvVarSource", "secretKeyRef") => Emitted { from: "machines.*.inputs" },
        ("SecretKeySelector", "name" | "key") => Emitted {
            from: "machines.*.inputs (the inputs' Secret)",
        },
        ("SecretKeySelector", "optional") => Emitted {
            from: "inputs (an unset input is empty)",
        },
        ("EnvVarSource", "configMapKeyRef" | "fileKeyRef") => Equivalent {
            via: "`value`: environment values are written as they are",
        },
        ("EnvVarSource", "fieldRef" | "resourceFieldRef") => API,
        _ => return None,
    })
}

fn security(field: &str) -> Option<Support> {
    Some(match field {
        "privileged" => Emitted { from: "machines.*.privileged" },
        "readOnlyRootFilesystem" => Emitted { from: "machines.*.read_only" },
        "runAsUser" | "runAsGroup" => Emitted {
            from: "(the volumes' seeds run as root, to copy any file)",
        },
        "windowsOptions" => WINDOWS,
        "allowPrivilegeEscalation" | "appArmorProfile" | "capabilities" | "procMount" | "runAsNonRoot" | "seLinuxOptions" | "seccompProfile" => CONTAINER,
        _ => return None,
    })
}

fn pod_settings(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("PodSecurityContext", "sysctls") | ("Sysctl", "name" | "value") => Emitted { from: "machines.*.sysctls" },
        ("PodSecurityContext", "runAsUser" | "runAsGroup") => Equivalent {
            via: "each container's `securityContext`",
        },
        ("PodSecurityContext", "windowsOptions") => WINDOWS,
        ("PodSecurityContext", _) => CONTAINER,
        ("PodDNSConfig", "nameservers") => Emitted {
            from: "machines.*.dns.servers",
        },
        ("PodDNSConfig", "searches") => Emitted { from: "machines.*.dns.search" },
        ("PodDNSConfigOption", "name" | "value") => Emitted {
            from: "machines.*.dns.options (`name` or `name:value`)",
        },
        ("PodDNSConfig", "options") => Emitted {
            from: "machines.*.dns.options (with the cluster's resolver unless the machine has servers of its own)",
        },
        _ => return None,
    })
}

fn affinity(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("Affinity", "podAffinity")
        | ("PodAffinity", "requiredDuringSchedulingIgnoredDuringExecution")
        | ("PodAffinityTerm", "labelSelector" | "topologyKey") => Emitted {
            from: "volumes (a machine sharing a volume runs on its owner's node)",
        },
        ("Affinity", "nodeAffinity" | "podAntiAffinity")
        | ("PodAffinity", "preferredDuringSchedulingIgnoredDuringExecution")
        | ("PodAffinityTerm", "matchLabelKeys" | "mismatchLabelKeys" | "namespaceSelector" | "namespaces") => SCHEDULING,
        _ => return None,
    })
}
