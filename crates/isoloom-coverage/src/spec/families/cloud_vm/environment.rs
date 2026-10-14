//! The cloud VM output's cells for the environment's fields: networks, reach, provisioning, checks.

use crate::spec::status::Status::{Done, NotApplicable, Partial, Planned};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Done {
            how: "default tags on every resource and resource names",
            proof: "segmented",
        },
    ),
    (
        "networks.*.cidr",
        Partial {
            how: "one VPC, a subnet per network with its exact range",
            gap: "subnets smaller than the cloud's minimum (/28 on AWS); offsets .1 to .3 are the cloud's",
            proof: "segmented",
        },
    ),
    (
        "networks.*.tc.delay",
        Planned {
            note: "netem on each VM's interface, as on Proxmox",
        },
    ),
    (
        "networks.*.tc.jitter",
        Planned {
            note: "netem on each VM's interface, as on Proxmox",
        },
    ),
    (
        "networks.*.tc.loss",
        Planned {
            note: "netem on each VM's interface, as on Proxmox",
        },
    ),
    (
        "networks.*.tc.rate",
        Planned {
            note: "netem on each VM's interface, as on Proxmox",
        },
    ),
    (
        "networks.*.docker.cidr",
        NotApplicable {
            why: "Docker only: VMs use `cidr` as written",
        },
    ),
    (
        "networks.*.vlans.*.cidr",
        Partial {
            how: "a subnet per VLAN; a VM on several VLANs gets an interface on each",
            gap: "802.1Q trunks: clouds don't carry tags",
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
        Partial {
            how: "as `networks.*.internet`, per VLAN",
            gap: "Windows machines stay online",
            proof: "vlan-office",
        },
    ),
    (
        "networks.*.internet",
        Partial {
            how: "new connections leaving the environment blocked in the guest once provisioned",
            gap: "Windows machines stay online",
            proof: "segmented",
        },
    ),
    (
        "networks.*.gateway",
        Partial {
            how: "the gateway VM at the network's router address (the cloud keeps .1), forwarding, its source/destination checks off, and route tables sending traffic between the network and the others through it both ways",
            gap: "the network's machines reach the internet at their own public addresses, not through the gateway",
            proof: "edge-firewall",
        },
    ),
    (
        "reach[].from",
        Done {
            how: "security groups: the `to` network's machines admit the `from` network's block",
            proof: "segmented",
        },
    ),
    (
        "reach[].to",
        Done {
            how: "the security group of each machine on that network",
            proof: "segmented",
        },
    ),
    (
        "reach[].ports",
        Done {
            how: "ingress rules per port, TCP and UDP (every port when none)",
            proof: "segmented",
        },
    ),
    (
        "inputs",
        Done {
            how: "a sensitive `inputs` variable, given at apply",
            proof: "supplier-portal-api",
        },
    ),
    ("secrets", crate::spec::status::SECRETS_ON_VMS),
    (
        "provision[].ansible",
        Done {
            how: "run from a controller instance with an interface on every network, once every machine is set up",
            proof: "ansible-pair",
        },
    ),
    (
        "provision[].inventory",
        Done {
            how: "more inventory files, next to the one Isoloom writes",
            proof: "ansible-pair",
        },
    ),
    (
        "provision[].groups",
        Done {
            how: "groups in the inventory Isoloom writes",
            proof: "ansible-pair",
        },
    ),
    (
        "provision[].vars",
        Done {
            how: "extra variables (`-e`)",
            proof: "ansible-pair",
        },
    ),
    (
        "provision[].host_vars",
        Done {
            how: "on each machine's line of the inventory Isoloom writes",
            proof: "ansible-pair",
        },
    ),
    (
        "controller",
        Partial {
            how: "the controller instance's type from `memory_mb`",
            gap: "`cpus` and `disk_gb`",
            proof: "ansible-pair",
        },
    ),
    (
        "provision[].requirements",
        Done {
            how: "Galaxy collections and roles installed first",
            proof: "ansible-pair",
        },
    ),
    (
        "checks[]",
        Partial {
            how: "a runner per position, run over SSH on its machine by `isoloom test cloud-vm`",
            gap: "a check with no `from` and no access machine runs only when a controller exists",
            proof: PROBE,
        },
    ),
    (
        "checks[].name",
        Done {
            how: "the runner's PASS/FAIL line",
            proof: "segmented",
        },
    ),
    (
        "checks[].from",
        Done {
            how: "the runner on that machine",
            proof: "segmented",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from the machine, retried for `wait`",
            proof: "segmented",
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from the machine",
            proof: "segmented",
        },
    ),
    ("checks[].method", Done { how: "curl -X", proof: PROBE }),
    ("checks[].headers", Done { how: "curl -H", proof: PROBE }),
    (
        "checks[].body",
        Done {
            how: "curl --data-binary",
            proof: PROBE,
        },
    ),
    (
        "checks[].contains",
        Done {
            how: "the response body or the first 5s read from the connection, searched for the text",
            proof: PROBE,
        },
    ),
    (
        "checks[].follow",
        Done {
            how: "redirects followed, the final status compared",
            proof: PROBE,
        },
    ),
    (
        "checks[].send",
        Done {
            how: "a line written to the connection before reading (nc else bash)",
            proof: PROBE,
        },
    ),
    (
        "checks[].exec",
        Done {
            how: "the command in the machine's own shell",
            proof: "tuned-web",
        },
    ),
    (
        "checks[].script",
        Done {
            how: "sh, from /opt/isoloom on the runner's machine",
            proof: PROBE,
        },
    ),
    (
        "checks[].expect",
        Done {
            how: "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp); text (exec)",
            proof: "segmented",
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
            note: "a tool VM on every network",
        },
    ),
    (
        "groups",
        Done {
            how: "folded into their members; Ansible inventory groups",
            proof: "ansible-pair",
        },
    ),
];
