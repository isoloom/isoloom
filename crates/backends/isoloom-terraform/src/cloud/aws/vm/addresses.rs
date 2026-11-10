//! A machine's several addresses on one network (`lan: [10, 11]`) on AWS: the VPC drops what
//! an interface sends or receives at an address it wasn't given, so each further address is a
//! secondary private address of the interface (`secondary_private_ips` on an instance with one
//! interface, the ordered `private_ip_list` of a network interface), and a whole aligned /28 of
//! them is one IPv4 prefix (`ipv4_prefixes`), which takes one of the interface's slots. The
//! instance's own set-up puts them on the interface. An instance type has a number of addresses
//! per interface (`Size::addresses`): a machine that needs more than its size gives gets a
//! larger type, and one no type takes is refused.

use std::net::Ipv4Addr;

use crate::hcl_builder::{BlockBuilder, boolean, list, string};
use isoloom_plan::MachinePlan;

/// The further addresses of a machine on one network: those AWS takes one by one, and the /28
/// prefixes it takes whole.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Split {
    /// Each a secondary private address.
    pub(super) single: Vec<Ipv4Addr>,
    /// Aligned /28 blocks every address of which is the machine's, as `a.b.c.d/28`.
    pub(super) prefixes: Vec<String>,
}

/// The further addresses of `mp` on `network`, split (`prefixes`: whether whole /28s become
/// prefixes; a Windows instance has a single interface, which takes no prefix there).
pub(super) fn split(mp: &MachinePlan, network: &str, prefixes: bool) -> Split {
    let more = mp.more_addresses.get(network).cloned().unwrap_or_default();
    let mut out = Split::default();
    let block = |a: &Ipv4Addr| u32::from(*a) & !0xf;
    let whole = |b: u32| (b..b + 16).all(|a| more.contains(&Ipv4Addr::from(a)));
    for a in &more {
        if prefixes && whole(block(a)) {
            let p = format!("{}/28", Ipv4Addr::from(block(a)));
            if !out.prefixes.contains(&p) {
                out.prefixes.push(p);
            }
        } else {
            out.single.push(*a);
        }
    }
    out
}

/// Whether a Linux machine needs its interfaces as network interfaces of their own: on several
/// networks, or with a prefix (which only a network interface takes).
pub(super) fn needs_interfaces(mp: &MachinePlan) -> bool {
    mp.addresses.len() > 1 || mp.addresses.keys().any(|n| !split(mp, n, true).prefixes.is_empty())
}

/// An instance with one interface: its further addresses on its network, when it has some.
pub(super) fn on_instance(instance: BlockBuilder, mp: &MachinePlan, network: &str) -> BlockBuilder {
    let s = split(mp, network, false);
    if s.single.is_empty() {
        return instance;
    }
    instance.add_attribute(("secondary_private_ips", list(s.single.iter().map(|a| string(a.to_string())))))
}

/// A network interface at `primary`, with the machine's further addresses on `network` after
/// it (in order, so the first stays the primary one) and its prefixes.
pub(super) fn on_interface(nic: BlockBuilder, mp: Option<&MachinePlan>, network: &str, primary: &str) -> BlockBuilder {
    let s = mp.map(|mp| split(mp, network, true)).unwrap_or_default();
    if s.single.is_empty() && s.prefixes.is_empty() {
        return nic.add_attribute(("private_ips", list([string(primary)])));
    }
    let addresses = std::iter::once(primary.to_owned()).chain(s.single.iter().map(ToString::to_string));
    let nic = nic
        .add_attribute(("private_ip_list_enabled", boolean(true)))
        .add_attribute(("private_ip_list", list(addresses.map(string))));
    if s.prefixes.is_empty() {
        return nic;
    }
    nic.add_attribute(("ipv4_prefixes", list(s.prefixes.iter().map(|p| string(p.as_str())))))
}

/// The addresses (prefixes counted once) the busiest interface of `mp` holds, which its
/// instance type must take (a Windows instance's single interface takes no prefix).
pub(super) fn busiest(mp: &MachinePlan) -> usize {
    let prefixes = !mp.is_windows();
    mp.addresses
        .keys()
        .map(|n| {
            let s = split(mp, n, prefixes);
            1 + s.single.len() + s.prefixes.len()
        })
        .max()
        .unwrap_or(1)
}

/// Why AWS can't give machine `name` its addresses: one of the first four of a subnet, which AWS
/// keeps. (An interface holding more than its instance type takes is the size's refusal.)
pub(super) fn refusal(name: &str, mp: &MachinePlan) -> Option<String> {
    let kept = mp.more_addresses.iter().find(|(_, more)| more.iter().any(|a| a.octets()[3] <= 3));
    kept.map(|(net, _)| format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    fn plan(octets: &str) -> MachinePlan {
        let yaml = format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: {octets} }}, os: debian@12, app: {{ steps: [{{ run: s.sh }}] }} }}\n"
        );
        crate::testing::planned(&yaml, Target::CloudVm).machines["web"].clone()
    }

    #[test]
    fn a_whole_aligned_block_is_one_prefix() {
        let all: Vec<String> = (16..32).map(|o| o.to_string()).collect();
        let mp = plan(&format!("[10, 11, {}]", all.join(", ")));
        let s = split(&mp, "lan", true);
        assert_eq!(s.single, ["10.0.0.11".parse::<Ipv4Addr>().unwrap()]);
        assert_eq!(s.prefixes, ["10.0.0.16/28"]);
        assert!(needs_interfaces(&mp));
        assert_eq!(busiest(&mp), 3);
        assert_eq!(split(&mp, "lan", false).single.len(), 17);
    }

    #[test]
    fn the_first_addresses_of_a_subnet_are_refused() {
        let mp = plan("[10, 11, 12]");
        assert!(!needs_interfaces(&mp));
        assert_eq!(busiest(&mp), 3);
        assert!(refusal("web", &mp).is_none());
        assert!(refusal("web", &plan("[10, 3]")).unwrap().contains("(.1 to .3)"));
    }
}
