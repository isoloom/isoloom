//! A network bridged to an interface of the host: the machines join the host's own LAN.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a network is bridged to the host (see [`super::Network::bridged`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bridged {
    /// The host interface, as the target names it (VirtualBox: `en0: Wi-Fi`; Proxmox: a bridge
    /// like `vmbr0`; ESXi: a physical NIC like `vmnic1` that no vSwitch holds, which gets a
    /// vSwitch of its own). Without it, Vagrant asks when several interfaces could do, Proxmox
    /// uses its uplink bridge, and ESXi its LAN vSwitch (`uplink_vswitch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// The machines take their address from the LAN's DHCP server instead of a fixed one in
    /// `cidr`; each machine lists the network with `0` (`networks: { home: 0 }`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dhcp: bool,
    /// With `dhcp`: the machines' default route goes through the LAN's own router (the one its
    /// DHCP server gives) instead of the environment's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default_route: bool,
    /// The VLAN tag on the bridge (1-4094), where the target tags a VM's interface (Proxmox; on
    /// ESXi, the port group's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,
}
