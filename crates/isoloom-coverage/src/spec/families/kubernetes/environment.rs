//! The Kubernetes output's cells for the environment's fields: networks, reach, provisioning, checks.

use crate::spec::status::Status::{Core, Done, NotApplicable, Partial, Planned};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Done {
            how: "the namespace `isoloom-<name>` and the chart's name (Helm: the release's namespace)",
            proof: "hello-stack",
        },
    ),
    (
        "networks.*.cidr",
        Partial {
            how: "a network label on its pods and a NetworkPolicy letting its members reach each other",
            gap: "the block itself: Kubernetes picks pod addresses",
            proof: "hello-stack",
        },
    ),
    (
        "networks.*.tc.delay",
        Planned {
            note: "needs a router in the path, which a cluster's pod network doesn't have",
        },
    ),
    (
        "networks.*.tc.jitter",
        Planned {
            note: "needs a router in the path, which a cluster's pod network doesn't have",
        },
    ),
    (
        "networks.*.tc.loss",
        Planned {
            note: "needs a router in the path, which a cluster's pod network doesn't have",
        },
    ),
    (
        "networks.*.tc.rate",
        Planned {
            note: "needs a router in the path, which a cluster's pod network doesn't have",
        },
    ),
    (
        "networks.*.docker.cidr",
        NotApplicable {
            why: "Docker only: Kubernetes picks pod addresses",
        },
    ),
    (
        "networks.*.vlans.*.cidr",
        Partial {
            how: "a network (label and NetworkPolicy) per VLAN (`<lan>-vlan<id>`); a machine on several VLANs carries each one's label",
            gap: "802.1Q trunks and the VLAN's block",
            proof: "vlan-office",
        },
    ),
    (
        "networks.*.switch",
        NotApplicable {
            why: "switch appliances run on the Docker targets only",
        },
    ),
    (
        "networks.*.vlans.*.internet",
        Done {
            how: "as `networks.*.internet`, per VLAN (the LAN's by default)",
            proof: "vlan-office",
        },
    ),
    (
        "networks.*.internet",
        Done {
            how: "an egress NetworkPolicy keeping offline pods inside the environment (and DNS to kube-system)",
            proof: "segmented",
        },
    ),
    (
        "networks.*.gateway",
        Planned {
            note: "routing a network through a pod needs a CNI that lets pods route",
        },
    ),
    (
        "reach[].from",
        Done {
            how: "a NetworkPolicy per rule letting the `from` network's pods in (with a CNI that enforces policies)",
            proof: "segmented",
        },
    ),
    (
        "reach[].to",
        Done {
            how: "the policy's pod selector (the `to` network's members)",
            proof: "segmented",
        },
    ),
    (
        "reach[].ports",
        Done {
            how: "the policy's ports, TCP and UDP each (every port when none)",
            proof: "segmented",
        },
    ),
    (
        "inputs",
        Done {
            how: "the optional keys of the `isoloom-inputs` Secret (Helm: `inputs.<NAME>` values)",
            proof: "supplier-portal-api",
        },
    ),
    (
        "provision[].ansible",
        Planned {
            note: "a controller pod running the playbooks",
        },
    ),
    (
        "provision[].inventory",
        Planned {
            note: "with `provision[].ansible`",
        },
    ),
    (
        "provision[].groups",
        Planned {
            note: "with `provision[].ansible`",
        },
    ),
    (
        "provision[].vars",
        Planned {
            note: "with `provision[].ansible`",
        },
    ),
    (
        "provision[].host_vars",
        Planned {
            note: "with `provision[].ansible`",
        },
    ),
    (
        "controller",
        NotApplicable {
            why: "no controller: `provision:` is refused",
        },
    ),
    (
        "provision[].requirements",
        Planned {
            note: "with `provision[].ansible`",
        },
    ),
    (
        "checks[]",
        Done {
            how: "a Job per position (its runner from a ConfigMap; Helm: `helm test` Pods)",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].name",
        Done {
            how: "the runner's PASS/FAIL line",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].from",
        Done {
            how: "the Job carries that machine's network labels, so the same policies apply",
            proof: "segmented",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from the Job, machines by Service name, retried for `wait`",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from the Job",
            proof: "segmented",
        },
    ),
    (
        "checks[].method",
        Done {
            how: "curl -X",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].headers",
        Done {
            how: "curl -H",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].body",
        Done {
            how: "curl --data-binary",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].contains",
        Done {
            how: "the response body or the first 5s read from the connection, searched for the text",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].follow",
        Done {
            how: "redirects followed, the final status compared",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].send",
        Done {
            how: "a line written to the connection before reading (nc else bash)",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].exec",
        Done {
            how: "in the machine itself, through `kubectl exec` (not in the Helm tests)",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].script",
        Done {
            how: "sh, from the `isoloom-scripts` ConfigMap",
            proof: PROBE,
        },
    ),
    (
        "checks[].expect",
        Done {
            how: "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp); text (exec)",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].wait",
        Done {
            how: "retried every 2s until it passes or the time is up",
            proof: "segmented",
        },
    ),
    (
        "tools",
        Planned {
            note: "a pod per tool on every network",
        },
    ),
    (
        "groups",
        Core {
            note: "folded into their members before generation",
        },
    ),
];
