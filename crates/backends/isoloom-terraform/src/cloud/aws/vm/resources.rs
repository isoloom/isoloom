//! The AWS resources the VM module declares and reads, each named by one function, so a block
//! and every reference to it agree on its address.

use crate::module::ResourceRef;

/// The controller's name among the machines' resources (its security group, instance,
/// interfaces and Elastic IP).
pub(super) const CONTROLLER: &str = "isoloom_controller";

/// The environment's VPC.
pub(super) fn vpc() -> ResourceRef {
    ResourceRef::new("aws_vpc", "env")
}

/// The VPC's internet gateway.
pub(super) fn internet_gateway() -> ResourceRef {
    ResourceRef::new("aws_internet_gateway", "env")
}

/// Network `net`'s block added to the VPC (every network's but the first).
pub(super) fn cidr_association(net: &str) -> ResourceRef {
    ResourceRef::new("aws_vpc_ipv4_cidr_block_association", net)
}

/// Network `net`'s subnet.
pub(super) fn subnet(net: &str) -> ResourceRef {
    ResourceRef::new("aws_subnet", net)
}

/// Network `net`'s subnet joined to its route table.
pub(super) fn route_table_association(net: &str) -> ResourceRef {
    ResourceRef::new("aws_route_table_association", net)
}

/// A route table a subnet takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteTable<'a> {
    /// The environment's, to the internet.
    Env,
    /// The networks without internet once the run is sealed.
    Sealed,
    /// A network's own, when a machine routes it.
    Own(&'a str),
}

/// Route table `table`.
pub(super) fn route_table(table: RouteTable<'_>) -> ResourceRef {
    let name = match table {
        RouteTable::Env => "env",
        RouteTable::Sealed => "sealed",
        RouteTable::Own(net) => net,
    };
    ResourceRef::new("aws_route_table", name)
}

/// Machine `machine`'s security group (the controller's: [`CONTROLLER`]).
pub(super) fn security_group(machine: &str) -> ResourceRef {
    ResourceRef::new("aws_security_group", machine)
}

/// Machine `machine`'s instance.
pub(super) fn instance(machine: &str) -> ResourceRef {
    ResourceRef::new("aws_instance", machine)
}

/// Machine `machine`'s network interface on network `net` (a machine on several networks).
pub(super) fn interface(machine: &str, net: &str) -> ResourceRef {
    ResourceRef::new("aws_network_interface", &interface_name(machine, net))
}

/// Interface [`interface`]`(machine, net)` attached to its instance.
pub(super) fn interface_attachment(machine: &str, net: &str) -> ResourceRef {
    ResourceRef::new("aws_network_interface_attachment", &interface_name(machine, net))
}

/// Machine `machine`'s Elastic IP (a machine on several networks).
pub(super) fn eip(machine: &str) -> ResourceRef {
    ResourceRef::new("aws_eip", machine)
}

/// The environment's private DNS zone (`dns.private`).
pub(super) fn route53_zone() -> ResourceRef {
    ResourceRef::new("aws_route53_zone", "private")
}

/// The private zone's record for name `name`.
pub(super) fn route53_record(name: &str) -> ResourceRef {
    ResourceRef::new("aws_route53_record", name)
}

/// The name an interface and its attachment share: `<machine>_<network>`.
fn interface_name(machine: &str, net: &str) -> String {
    format!("{}_{}", crate::module::ident(machine), crate::module::ident(net))
}
