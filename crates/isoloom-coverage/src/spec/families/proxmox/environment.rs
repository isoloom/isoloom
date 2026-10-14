//! The Proxmox output's cells for the environment's fields: networks, reach, provisioning, checks.

use crate::spec::status::Status;
use crate::spec::status::Status::{Done, NotApplicable, Planned};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Done {
            how: "tags and VM names (with the slot)",
            proof: "segmented",
        },
    ),
    (
        "networks.*.cidr",
        Done {
            how: "an SDN VNet per network in the environment's own zone",
            proof: "segmented",
        },
    ),
    (
        "networks.*.tc.delay",
        Done {
            how: "netem on each VM's interface into the network, re-applied at boot",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.jitter",
        Done {
            how: "netem on each VM's interface into the network, re-applied at boot",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.loss",
        Done {
            how: "netem on each VM's interface into the network, re-applied at boot",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.rate",
        Done {
            how: "netem on each VM's interface into the network, re-applied at boot",
            proof: "slow-link",
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
        Done {
            how: "an SDN VNet per VLAN; a VM on several VLANs of the LAN gets an 802.1Q trunk inside it (`<lan>.<id>`)",
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
            how: "new connections leaving the environment blocked once provisioned",
            proof: "segmented",
        },
    ),
    (
        "networks.*.gateway",
        Planned {
            note: "a gateway VM routing its network",
        },
    ),
    (
        "reach[].from",
        Done {
            how: "a router VM filtering with nftables",
            proof: "segmented",
        },
    ),
    (
        "reach[].to",
        Done {
            how: "the router's rules",
            proof: "segmented",
        },
    ),
    (
        "reach[].ports",
        Done {
            how: "the router's rules, per port",
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
    (
        "provision[].ansible",
        Done {
            how: "run from a controller VM on every network, once every machine is up",
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
        Done {
            how: "the controller VM's resources (it boots the Debian cloud image: `image` names a Vagrant box)",
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
        Done {
            how: "a runner per position (.isoloom/proxmox/checks/), piped over SSH through the router by `isoloom test proxmox`",
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
            how: "the runner on that machine",
            proof: "segmented",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from the machine, retried for `wait`",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from the machine",
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
            how: "the response body (http, curl) or the first 5s read from the connection (tcp, nc else bash), searched for the text",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].follow",
        Done {
            how: "redirects followed (curl -L --max-redirs 5; wget, which follows anyway, reads the last of at most 6 answers), the final status compared",
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
            how: "the command in the machine's own shell",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].script",
        Done {
            how: "sh, from /opt/isoloom",
            proof: "slow-link",
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
