//! Every output's cells for a network's IPv6 side (`networks.*.ipv6`), in one place: dual-stack
//! networks, the addresses a target hands out, IPv6-only networks.

use crate::spec::status::Status::{Done, Planned};
use crate::spec::status::{PROBE, Status};

/// The Compose file's.
pub const COMPOSE: &[(&str, Status)] = &[
    (
        "networks.*.ipv6.cidr",
        Done {
            how: "the network's IPv6 subnet (`enable_ipv6`), each machine's `ipv6_address`, the router's IPv6 address, rules and routes",
            proof: "dual-stack",
        },
    ),
    (
        "networks.*.ipv6.addresses",
        Done {
            how: "`auto`: no `ipv6_address`, Docker hands one out of a range clear of the fixed addresses",
            proof: PROBE,
        },
    ),
    (
        "networks.*.ipv6.only",
        Done {
            how: "`enable_ipv4: false` (Docker Engine 28 or later), no `ipv4_address`; probes use the IPv6 addresses",
            proof: PROBE,
        },
    ),
];

/// The Kubernetes manifests'.
pub const KUBERNETES: &[(&str, Status)] = &[
    (
        "networks.*.ipv6.cidr",
        Done {
            how: "the Services ask for both IP families (`PreferDualStack`); the cluster picks pod addresses",
            proof: "dual-stack",
        },
    ),
    (
        "networks.*.ipv6.addresses",
        Done {
            how: "the cluster picks pod addresses either way",
            proof: PROBE,
        },
    ),
    (
        "networks.*.ipv6.only",
        Done {
            how: "`SingleStack` IPv6 Services for machines on IPv6-only networks alone",
            proof: PROBE,
        },
    ),
];

/// The VM outputs' that run the set-up steps (Vagrant, Proxmox): `vm_how` says how the
/// network carries the block.
const fn vm(cidr: Status) -> [(&'static str, Status); 3] {
    [
        ("networks.*.ipv6.cidr", cidr),
        (
            "networks.*.ipv6.addresses",
            Planned {
                note: "nothing on a VM network hands IPv6 addresses out (no router advertisement): `static` only",
            },
        ),
        (
            "networks.*.ipv6.only",
            Planned {
                note: "the router, the set-up and the transport need IPv4 on every network",
            },
        ),
    ]
}

/// Vagrant's.
pub const VAGRANT: &[(&str, Status)] = &vm(Done {
    how: "the libvirt network carries the block (`libvirt__ipv6_*`, `libvirt__guest_ipv6`); a set-up step gives each VM its address and routes on every provider, the router its IPv6 side (not run on a real host)",
    proof: "dual-stack",
});

/// Proxmox's.
pub const PROXMOX: &[(&str, Status)] = &vm(Done {
    how: "a set-up step gives each VM its address and routes; the router gets its IPv6 address and rules (not run on a real host)",
    proof: "dual-stack",
});

/// The clouds' (each cloud that can't keep the spec's block refuses with its own reason).
pub const CLOUD_VM: &[(&str, Status)] = &[
    (
        "networks.*.ipv6.cidr",
        Done {
            how: "the cloud network keeps the spec's block: an address space and subnet prefix on Azure, a private prefix of the VCN and its subnet's on Oracle Cloud; each interface its derived address, rules mirroring the IPv4 ones (not run on a real host)",
            proof: "dual-stack",
        },
    ),
    (
        "networks.*.ipv6.addresses",
        Done {
            how: "`auto` on Oracle Cloud: the VNIC asks for an address in the subnet's prefix, and the set-up reads the one Oracle Cloud picked back from Terraform (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "networks.*.ipv6.only",
        Planned {
            note: "the set-up and the transport need IPv4",
        },
    ),
];

/// The existing hosts'.
pub const EXTERNAL: &[(&str, Status)] = &[
    (
        "networks.*.ipv6.cidr",
        Done {
            how: "nothing to build: the hosts have their addresses; probes use the IPv4 ones",
            proof: PROBE,
        },
    ),
    (
        "networks.*.ipv6.addresses",
        Done {
            how: "nothing to build: the hosts have their addresses",
            proof: PROBE,
        },
    ),
    (
        "networks.*.ipv6.only",
        Planned {
            note: "the probes would need the hosts' IPv6 addresses, which the spec doesn't give",
        },
    ),
];

#[cfg(test)]
mod tests {
    use super::{PROXMOX, VAGRANT, vm};

    /// The tables are built at compile time; building them again at run time proves the helper
    /// gives the same rows (and lets line coverage see it run).
    #[test]
    fn the_vm_tables_are_the_helper_s_rows() {
        assert_eq!(vm(VAGRANT[0].1).as_slice(), VAGRANT);
        assert_eq!(vm(PROXMOX[0].1).as_slice(), PROXMOX);
    }
}
