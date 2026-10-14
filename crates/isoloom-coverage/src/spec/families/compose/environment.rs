//! The Docker Compose output's cells for the environment's fields: networks, reach, provisioning, checks.

use crate::spec::status::Status;
use crate::spec::status::Status::{Core, Done, Planned};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Done {
            how: "the Compose project name",
            proof: "hello-stack",
        },
    ),
    (
        "networks.*.cidr",
        Done {
            how: "a Compose network with that subnet",
            proof: "hello-stack",
        },
    ),
    (
        "networks.*.tc.delay",
        Done {
            how: "netem on the router's interface into the network and each machine's own (its network sidecar)",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.jitter",
        Done {
            how: "netem on the router's interface into the network and each machine's own (its network sidecar)",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.loss",
        Done {
            how: "netem on the router's interface into the network and each machine's own (its network sidecar)",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.tc.rate",
        Done {
            how: "netem on the router's interface into the network and each machine's own (its network sidecar)",
            proof: "slow-link",
        },
    ),
    (
        "networks.*.docker.cidr",
        Done {
            how: "the network's block on Docker (others outside 10.0.0.0/8 move there automatically)",
            proof: "air-gapped",
        },
    ),
    (
        "networks.*.vlans.*.cidr",
        Done {
            how: "a Compose network per VLAN (`<lan>-vlan<id>`); a machine on several VLANs of the LAN gets one 802.1Q trunk (`<lan>.<id>` subinterfaces) through a switch container",
            proof: "vlan-office",
        },
    ),
    (
        "networks.*.switch",
        Done {
            how: "the LAN's switch appliance instead of the Linux switch container: an access port per VLAN network, a trunk port per trunk link, VLANs and port modes in its startup configuration",
            proof: "cisco-switch",
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
            how: "no default route for offline machines (Docker's internal network when nothing routes)",
            proof: "segmented",
        },
    ),
    (
        "networks.*.gateway",
        Done {
            how: "the gateway machine at .1 with forwarding; machines behind it route through it",
            proof: "edge-firewall",
        },
    ),
    (
        "reach[].from",
        Done {
            how: "a router container filtering with nftables",
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
            how: "variables read from the shell at start",
            proof: "supplier-portal-api",
        },
    ),
    (
        "secrets",
        crate::spec::status::Status::Done {
            how: "top-level `secrets`, each read by Compose from `ISOLOOM_SECRET_<NAME>`, which `isoloom run` sets from the store (never in the file)",
            proof: crate::spec::status::PROBE,
        },
    ),
    (
        "provision[].ansible",
        Planned {
            note: "environment-level provisioning on containers (a controller container)",
        },
    ),
    (
        "provision[].inventory",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "provision[].groups",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "provision[].vars",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "provision[].host_vars",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "controller",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "provision[].requirements",
        Planned {
            note: "with environment-level provisioning on containers",
        },
    ),
    (
        "checks[]",
        Done {
            how: "a `check` profile: a runner per position (a sh script next to the Compose file), scripts from the project mounted read-only",
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
            how: "the runner in that machine's network namespace (a stand-in's when the runner supplies it)",
            proof: "segmented",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from the position, retried for `wait`",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from the position",
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
            how: "in the machine itself: its own runner piped to `docker compose exec -T <machine> sh -s`",
            proof: "hello-stack",
        },
    ),
    (
        "checks[].script",
        Done {
            how: "sh, from the project mounted read-only",
            proof: "edge-firewall",
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
        Done {
            how: "a container per tool on every network at the reserved addresses (`shell`: netshoot)",
            proof: "slow-link",
        },
    ),
    (
        "groups",
        Core {
            note: "folded into their members before generation",
        },
    ),
];
