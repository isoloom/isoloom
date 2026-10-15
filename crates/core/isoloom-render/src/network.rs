//! The network pieces every target renders the same way: Isoloom's router, VLAN trunks, IPv6 on
//! a VM, and a VM's further addresses on a network.

pub mod ipv6;
pub mod more_addresses;
pub mod router;
pub mod trunks;
