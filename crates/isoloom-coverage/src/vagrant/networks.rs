//! The options of `config.vm.network`: forwarded ports, private networks (with the
//! provider-scoped `virtualbox__` and `libvirt__` options) and public networks.

use super::{HOST_TUNING, IMAGE_BOOTS, IPV6, MAC, NETWORKS, USER_SETUP, pairs, table};
use crate::{
    Format, Support,
    Support::{Emitted, Equivalent, NotPortable, Tooling},
};

/// The "Vagrant: networks" table.
pub(super) fn format() -> Format {
    let keys = pairs(NETWORKS)
        .into_iter()
        .map(|(kind, o)| {
            let s = match kind {
                "forwarded_port" => forwarded_port(o),
                "private_network" => private_network(o),
                "public_network" => public_network(o),
                _ => None,
            };
            (format!("config.vm.network {kind}: {o}"), s)
        })
        .collect();
    table(
        "Vagrant: networks",
        "Vagrant 2.4.9 (the networking docs and the providers' network code) and vagrant-libvirt 0.12.2 (its network options)",
        keys,
    )
}

fn public_network(o: &str) -> Option<Support> {
    Some(match o {
        "bridge" => Emitted {
            from: "networks.*.bridged.interface",
        },
        "ip" => Emitted {
            from: "machines.*.networks (a fixed address on a bridged network; DHCP otherwise)",
        },
        "use_dhcp_assigned_default_route" => Emitted {
            from: "networks.*.bridged.default_route",
        },
        "auto_config" => Equivalent {
            via: "networks.*.bridged (Vagrant always configures the bridged interface: a fixed address, or DHCP)",
        },
        _ => return None,
    })
}

fn forwarded_port(o: &str) -> Option<Support> {
    Some(match o {
        "guest" | "host" => Emitted {
            from: "machines.*.services[].publish",
        },
        "host_ip" => Emitted {
            from: "(the host's loopback: a published port isn't open to the LAN)",
        },
        "protocol" => Emitted {
            from: "machines.*.services[].protocol",
        },
        "id" | "disabled" => Emitted {
            from: "(a Windows box's own RDP forward, turned off so VMs don't compete for the host port)",
        },
        "auto_correct" => Tooling {
            note: "Vagrant moving a forward to a free host port: published ports are fixed",
        },
        "guest_ip" => Equivalent {
            via: "machines.*.services[].port (forwarded to the machine itself)",
        },
        _ => return None,
    })
}

/// DHCP on a network: machines get fixed addresses instead, the same on every target.
const FIXED_ADDRESSES: Support = Equivalent {
    via: "machines.*.networks (a fixed address on every target, instead of DHCP)",
};

fn private_network(o: &str) -> Option<Support> {
    if let Some(rest) = o.strip_prefix("libvirt__") {
        return libvirt(rest);
    }
    Some(match o {
        "ip" => Emitted {
            from: "machines.*.networks (fixed addresses)",
        },
        "netmask" => Emitted { from: "networks.*.cidr" },
        "auto_config" => Emitted {
            from: "(off for the hybrid target's Docker host: its containers take the addresses on those networks)",
        },
        "mac" => Emitted {
            from: "(the hybrid target's Docker host: a fixed MAC per network, so its macvlan networks find their adapter)",
        },
        "bridge" => Emitted {
            from: "(Hyper-V: each VM on the networks' private switches)",
        },
        "virtualbox__intnet" => Emitted {
            from: "networks (an internal network each: between the VMs only, no host adapter)",
        },
        "intnet" => Equivalent {
            via: "`virtualbox__intnet`, the same option scoped to VirtualBox",
        },
        "adapter" | "name" => Tooling {
            note: "which adapter and host network the provider uses",
        },
        "type" | "adapter_ip" | "dhcp_ip" | "dhcp_lower" | "dhcp_upper" => FIXED_ADDRESSES,
        "nic_type" => HOST_TUNING,
        _ => return None,
    })
}

fn libvirt(o: &str) -> Option<Support> {
    Some(match o {
        "network_name" => Emitted {
            from: "networks (a libvirt network each, named after the environment)",
        },
        "dhcp_enabled" => Emitted {
            from: "(off: machines get fixed addresses)",
        },
        "network_address" | "netmask" | "host_ip" => Equivalent {
            via: "`ip` and `netmask`: libvirt derives the network from them",
        },
        "dhcp_start" | "dhcp_stop" => FIXED_ADDRESSES,
        "dhcp_bootp_file" | "dhcp_bootp_server" | "tftp_root" => IMAGE_BOOTS,
        "forward_mode" => Equivalent {
            via: "networks.*.internet (out through Isoloom's router, the same on every target)",
        },
        "domain_name" => Equivalent { via: "machines.*.dns.domain" },
        "guest_ipv6" | "ipv6_address" | "ipv6_prefix" => IPV6,
        "mac" => MAC,
        "mtu" => NotPortable {
            why: "cloud networks set their own MTU",
        },
        "tunnel_type" | "tunnel_ip" | "tunnel_port" | "tunnel_local_ip" | "tunnel_local_port" => NotPortable {
            why: "links libvirt hosts together: other targets join machines on one network of their own",
        },
        "model_type" | "driver_name" | "driver_queues" | "bus" | "slot" => HOST_TUNING,
        "forward_device" => USER_SETUP,
        "adapter" | "iface_name" | "always_destroy" => Tooling {
            note: "how vagrant-libvirt names and cleans up its interfaces",
        },
        _ => return None,
    })
}
