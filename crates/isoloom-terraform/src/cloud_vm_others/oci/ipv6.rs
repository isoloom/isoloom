//! A machine's IPv6 address on each dual-stack network, on its VNIC there.
//!
//! - `static` (the default): the VNIC takes the address derived from the machine's octet in the
//!   subnet's prefix, and the plan's IPv6 step puts it on the interface, as on every VM target.
//! - `auto`: Oracle Cloud picks a free address in the subnet's prefix. The machine's set-up reads
//!   it back from Terraform (the primary VNIC's, which the provider derives from the VNIC when the
//!   module names none; a secondary VNIC's, from its `oci_core_vnic`) and puts it on the
//!   interface holding the machine's IPv4 address there, at every boot.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Template, attr, boolean, index, reference, string};
use isoloom_spec::model::Ipv6Addresses;
use isoloom_spec::validate::IPV6_PREFIX;

use crate::vm::res;

/// Whether `net` is dual-stack and leaves its machines' addresses to Oracle Cloud.
fn auto(env: &Environment, net: &str) -> bool {
    env.networks[net].ipv6.as_ref().is_some_and(|v| v.addresses == Ipv6Addresses::Auto)
}

/// A VNIC's IPv6 side on `net` (`create_vnic_details`): an address from the subnet's prefix,
/// the machine's derived one unless the network leaves it to Oracle Cloud.
pub fn vnic(env: &Environment, mp: &MachinePlan, net: &str, b: BlockBuilder) -> BlockBuilder {
    let Some(v6) = &env.networks[net].ipv6 else {
        return b;
    };
    let b = b.add_attribute(("assign_ipv6ip", boolean(true)));
    let Some(address) = mp.addresses6.get(net) else {
        return b;
    };
    b.add_block(
        Block::builder("ipv6address_ipv6subnet_cidr_pair_details")
            .add_attribute(("ipv6subnet_cidr", string(v6.cidr.as_str())))
            .add_attribute(("ipv6address", string(address.to_string())))
            .build(),
    )
}

/// The commands putting each address Oracle Cloud picked (`auto` networks) on its interface,
/// found by the machine's IPv4 address there: run after the extra interfaces are up.
pub fn at_boot(env: &Environment, name: &str, mp: &MachinePlan) -> Vec<Template> {
    let id = res(name);
    mp.addresses
        .iter()
        .enumerate()
        .filter(|(_, (net, _))| auto(env, net))
        .map(|(i, (net, v4))| {
            let address = if i == 0 {
                let details = index(reference(&format!("oci_core_instance.{id}.create_vnic_details")), 0);
                attr(index(attr(details, "ipv6address_ipv6subnet_cidr_pair_details"), 0), "ipv6address")
            } else {
                index(reference(&format!("data.oci_core_vnic.{id}_{}.ipv6addresses", res(net))), 0)
            };
            put(&v4.to_string(), address)
        })
        .collect()
}

/// `address` on the interface holding `v4`, IPv6 on there first.
fn put(v4: &str, address: Expression) -> Template {
    Template::text(format!(
        "IF=$(ip -o -4 addr show | grep -F ' inet {v4}/' | awk '{{print $2; exit}}'); sudo sysctl -q -w \"net.ipv6.conf.$IF.disable_ipv6=0\" && sudo ip -6 addr replace "
    ))
    .with_expr(address)
    .with_text(format!("/{IPV6_PREFIX} dev \"$IF\" nodad"))
}

/// Why a machine's derived address can't be had, if one can't: Oracle Cloud keeps the first four
/// addresses of a subnet's prefix (and its last four, which no octet reaches).
pub fn refusal(env: &Environment) -> Option<String> {
    crate::vm::vms(env).find_map(|(name, mp)| {
        let (net, a) = mp.addresses6.iter().find(|(_, a)| u128::from(**a) & u128::from(u64::MAX) <= 3)?;
        Some(format!(
            "machine `{name}`: Oracle Cloud keeps the first four IPv6 addresses of a subnet's prefix, and {a} on `{net}` is one of them"
        ))
    })
}
