//! Every rendering's cells for the networks outside the environment: an existing network joined
//! by name (`external`) and a network bridged to the host (`bridged`). A rendering that can't
//! join or bridge one says what it would take; its backend's refusal then gives the reason.

use crate::spec::status::Status::{self, Done, Planned};
use crate::spec::status::{EXISTING_HOST, PROBE};

/// What a bridged network is on a rendering that has none.
const NO_BRIDGE: &str = "a bridge to the host's LAN, which this output has no way to make";

/// The Compose file: an existing Docker network, by name. A container reaches the host's LAN
/// through a macvlan network made beforehand, named with `external`.
pub const COMPOSE: &[(&str, Status)] = &[
    (
        "networks.*.external",
        Done {
            how: "the existing Docker network (`external: true` and its `name`), joined at the machine's fixed address; Isoloom never creates or removes it",
            proof: PROBE,
        },
    ),
    ("networks.*.bridged.interface", COMPOSE_BRIDGE),
    ("networks.*.bridged.dhcp", COMPOSE_BRIDGE),
    ("networks.*.bridged.default_route", COMPOSE_BRIDGE),
    ("networks.*.bridged.vlan", COMPOSE_BRIDGE),
];

const COMPOSE_BRIDGE: Status = Planned {
    note: "a container reaches the host's LAN through a macvlan network made beforehand: name it with `external`",
};

/// Kubernetes: pods join the cluster's own network.
pub const KUBERNETES: &[(&str, Status)] = &[
    (
        "networks.*.external",
        Planned {
            note: "pods join the cluster's network, not one by name",
        },
    ),
    ("networks.*.bridged.interface", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.dhcp", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.default_route", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.vlan", Planned { note: NO_BRIDGE }),
];

/// The Vagrantfile: a public network bridged to the host's interface.
pub const VAGRANT: &[(&str, Status)] = &[
    (
        "networks.*.external",
        Planned {
            note: "an existing network by name: the VMs' private networks are Isoloom's own",
        },
    ),
    (
        "networks.*.bridged.interface",
        Done {
            how: "`public_network` with `bridge` (the interface as the provider names it), after the machine's own networks",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.dhcp",
        Done {
            how: "`public_network` without `ip`: the LAN's DHCP server gives the address",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.default_route",
        Done {
            how: "`use_dhcp_assigned_default_route`",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.vlan",
        Planned {
            note: "a VLAN tag on the bridged interface: Vagrant's public networks take none",
        },
    ),
];

/// The Proxmox module: a network device on an existing bridge, or on the uplink bridge.
pub const PROXMOX: &[(&str, Status)] = &[
    (
        "networks.*.external",
        Done {
            how: "a network device on the existing bridge, at the machine's fixed address (not run on a Proxmox server yet)",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.interface",
        Done {
            how: "the device's bridge (the uplink bridge when none is named)",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.dhcp",
        Done {
            how: "the device's cloud-init address `dhcp`",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.default_route",
        Done {
            how: "no gateway on the first network, so the LAN's DHCP route is the default",
            proof: PROBE,
        },
    ),
    (
        "networks.*.bridged.vlan",
        Done {
            how: "the device's `vlan_id`",
            proof: PROBE,
        },
    ),
];

/// The cloud modules: each makes its own VPC and firewall for the environment.
pub const CLOUD_VM: &[(&str, Status)] = &[
    (
        "networks.*.external",
        Planned {
            note: "an existing VPC or subnet by ID: each cloud's module makes its own VPC and firewall for the environment",
        },
    ),
    ("networks.*.bridged.interface", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.dhcp", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.default_route", Planned { note: NO_BRIDGE }),
    ("networks.*.bridged.vlan", Planned { note: NO_BRIDGE }),
];

/// Existing hosts are on their networks already.
pub const EXTERNAL: &[(&str, Status)] = &[
    ("networks.*.external", EXISTING_HOST),
    ("networks.*.bridged.interface", EXISTING_HOST),
    ("networks.*.bridged.dhcp", EXISTING_HOST),
    ("networks.*.bridged.default_route", EXISTING_HOST),
    ("networks.*.bridged.vlan", EXISTING_HOST),
];
