//! The external output's cells for the environment's fields: networks, reach, provisioning, checks.

use crate::spec::status::Status::{Core, Done, NotApplicable, Partial, Planned};
use crate::spec::status::{PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Core {
            note: "names the environment (status, registry); nothing is written on the hosts",
        },
    ),
    (
        "networks.*.cidr",
        Done {
            how: "the addresses the check runners probe (the hosts keep their own networks)",
            proof: "existing-hosts",
        },
    ),
    (
        "networks.*.tc.delay",
        Planned {
            note: "netem on the hosts' interfaces over SSH",
        },
    ),
    (
        "networks.*.tc.jitter",
        Planned {
            note: "netem on the hosts' interfaces over SSH",
        },
    ),
    (
        "networks.*.tc.loss",
        Planned {
            note: "netem on the hosts' interfaces over SSH",
        },
    ),
    (
        "networks.*.tc.rate",
        Planned {
            note: "netem on the hosts' interfaces over SSH",
        },
    ),
    (
        "networks.*.docker.cidr",
        NotApplicable {
            why: "Docker only: the hosts already have their addresses",
        },
    ),
    (
        "networks.*.vlans.*.cidr",
        Done {
            how: "the per-VLAN addresses the runners probe (no trunk is set up on the hosts)",
            proof: PROBE,
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
            how: "the derived \"no internet\" check, per VLAN",
            gap: "verified, not enforced",
            proof: PROBE,
        },
    ),
    (
        "networks.*.internet",
        Partial {
            how: "the derived \"no internet\" check in the machine's runner",
            gap: "verified, not enforced",
            proof: PROBE,
        },
    ),
    (
        "networks.*.gateway",
        NotApplicable {
            why: "the gateway host routes as it already does",
        },
    ),
    (
        "reach[].from",
        Partial {
            how: "derived open and blocked checks from each host",
            gap: "verified, not enforced: the lab's own firewall decides",
            proof: PROBE,
        },
    ),
    (
        "reach[].to",
        Partial {
            how: "the derived checks' targets",
            gap: "verified, not enforced",
            proof: PROBE,
        },
    ),
    (
        "reach[].ports",
        Partial {
            how: "the derived checks, per port",
            gap: "verified, not enforced",
            proof: PROBE,
        },
    ),
    (
        "inputs",
        Planned {
            note: "passed to the steps and playbooks",
        },
    ),
    ("secrets", crate::spec::status::SECRETS_ON_VMS),
    (
        "provision[].ansible",
        Done {
            how: "ansible-playbook run from here with the generated inventory, after the machines' own steps",
            proof: PROBE,
        },
    ),
    (
        "provision[].inventory",
        Done {
            how: "more `-i` files, after the generated inventory",
            proof: PROBE,
        },
    ),
    (
        "provision[].groups",
        Done {
            how: "groups in the generated inventory",
            proof: PROBE,
        },
    ),
    (
        "provision[].vars",
        Done {
            how: "extra variables (`-e`)",
            proof: PROBE,
        },
    ),
    (
        "provision[].host_vars",
        Done {
            how: "on each host's line of the inventory",
            proof: PROBE,
        },
    ),
    (
        "controller",
        NotApplicable {
            why: "playbooks run from this computer: there is no controller",
        },
    ),
    (
        "provision[].requirements",
        Planned {
            note: "Galaxy collections and roles installed before the playbooks",
        },
    ),
    (
        "checks[]",
        Partial {
            how: "a runner per host, piped over SSH by `isoloom test external`",
            gap: "checks with no machine position; script and playbook checks",
            proof: PROBE,
        },
    ),
    (
        "checks[].name",
        Done {
            how: "the runner's PASS/FAIL line",
            proof: "existing-hosts",
        },
    ),
    (
        "checks[].from",
        Done {
            how: "the runner on that host",
            proof: "existing-hosts",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from the host, retried for `wait`",
            proof: PROBE,
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from the host",
            proof: PROBE,
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
            how: "a line written to the connection before reading",
            proof: PROBE,
        },
    ),
    (
        "checks[].exec",
        Done {
            how: "the command in the host's own shell, over SSH",
            proof: "existing-hosts",
        },
    ),
    (
        "checks[].script",
        Planned {
            note: "run from /opt/isoloom on the host",
        },
    ),
    (
        "checks[].expect",
        Done {
            how: "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp); text (exec)",
            proof: "existing-hosts",
        },
    ),
    (
        "checks[].wait",
        Done {
            how: "retried every 2s until it passes or the time is up",
            proof: PROBE,
        },
    ),
    (
        "tools",
        NotApplicable {
            why: "no tool containers or VMs: the lab's hosts are its own",
        },
    ),
    (
        "groups",
        Done {
            how: "folded into their members; Ansible inventory groups",
            proof: PROBE,
        },
    ),
];
