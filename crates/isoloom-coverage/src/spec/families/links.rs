//! Every output's cells for a network's `mtu`, a machine's `rate_limit` and `docker.lxc`, in one
//! place: only the Proxmox module sets the first two (on its network devices) and runs the
//! third; every other output's backend refuses a spec with `mtu` or `rate_limit`.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, NotApplicable, Planned};

/// A network's MTU not carried yet: the backend refuses a spec that sets it.
const fn mtu(note: &'static str) -> (&'static str, Status) {
    ("networks.*.mtu", Planned { note })
}

/// A machine's bandwidth cap not applied yet: the backend refuses a spec that sets it.
const fn rate(note: &'static str) -> (&'static str, Status) {
    ("machines.*.rate_limit", Planned { note })
}

/// `docker.lxc` means nothing to an output that runs the machine as Docker would.
const NOT_LXC: (&str, Status) = (
    "machines.*.docker.lxc",
    NotApplicable {
        why: "Proxmox only: elsewhere the machine runs as its `docker:` or `vm:` says",
    },
);

/// The Compose file's.
pub const COMPOSE: &[(&str, Status)] = &[
    mtu("the networks' `driver_opts` MTU comes later"),
    rate("a container's bandwidth cap comes later (tc in its network namespace)"),
    NOT_LXC,
];

/// The Kubernetes manifests'.
pub const KUBERNETES: &[(&str, Status)] = &[
    mtu("the cluster's network plugin sets the MTU, not a manifest"),
    rate("a bandwidth annotation needs the cluster's bandwidth plugin; comes later"),
    NOT_LXC,
];

/// Vagrant's.
pub const VAGRANT: &[(&str, Status)] = &[
    mtu("a private network's MTU on each provider comes later"),
    rate("a VM's bandwidth cap (VirtualBox bandwidth groups, tc) comes later"),
    NOT_LXC,
];

/// The cloud VM core's.
pub const CLOUD_VM: &[(&str, Status)] = &[
    mtu("the cloud's network sets its own MTU"),
    rate("a VM's bandwidth cap inside it (tc) comes later"),
    NOT_LXC,
];

/// The existing hosts'.
pub const EXTERNAL: &[(&str, Status)] = &[
    mtu("Isoloom doesn't change an existing host's links"),
    rate("Isoloom doesn't change an existing host's links"),
    NOT_LXC,
];

/// The Proxmox module's.
pub const PROXMOX: &[(&str, Status)] = &[
    (
        "networks.*.mtu",
        Done {
            how: "`mtu` on every device on the network (the machines', the router's, the controller's), the SDN zone's at the largest (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.rate_limit",
        Done {
            how: "`rate_limit` (MB/s) on each of the machine's devices, a VM's or an LXC container's (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.docker.lxc",
        Done {
            how: "an LXC container from the image pulled as an OCI template (Proxmox VE 9.1 or later): its entrypoint, `environment`, addresses, the router as its gateway and resolver, the firewall; what a container can't carry is refused (not run on a real host)",
            proof: PROBE,
        },
    ),
];

/// What the ESXi module states apart from the Proxmox family it inherits from: it runs no LXC
/// container (its `mtu` and `rate_limit` cells come from its backend's refusals).
pub const ESXI: &[(&str, Status)] = &[NOT_LXC];
