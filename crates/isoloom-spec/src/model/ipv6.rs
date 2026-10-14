//! A network's IPv6 block: dual-stack beside its IPv4 `cidr`, or IPv6-only.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A network's IPv6 side (see [`super::Network::ipv6`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ipv6 {
    /// An IPv6 /64 block, unique local (`fd00:20::/64`) or global. A machine at octet 10 of
    /// the IPv4 block gets `<block>::10`: the octet's digits, read as the address's last group,
    /// so both addresses read alike.
    pub cidr: String,
    /// How machines get their IPv6 address: `static` (default), the one derived from their
    /// octet, or `auto`, the one the target hands out (Docker's address management, the
    /// cluster's, Oracle Cloud's). Isoloom's own addresses stay fixed either way.
    #[serde(default, skip_serializing_if = "Ipv6Addresses::is_static")]
    pub addresses: Ipv6Addresses,
    /// IPv6 only: the network carries no IPv4. Its `cidr` still numbers the machines (their
    /// octet), but no target puts it on the wire. Such a network has no `reach`, `gateway`,
    /// `tc` or internet: those go through Isoloom's IPv4 router or NAT.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only: bool,
}

/// How machines get their IPv6 address on a network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Ipv6Addresses {
    /// Derived from the machine's octet, the same on every target.
    #[default]
    Static,
    /// Handed out by the target.
    Auto,
}

impl Ipv6Addresses {
    /// Whether addresses are derived from the octets (the default, left out when written).
    #[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference.
    pub fn is_static(&self) -> bool {
        *self == Ipv6Addresses::Static
    }
}
