//! A machine's several addresses on one network (`lan: [10, 11]`) on AWS: the VPC drops what
//! an interface sends or receives at an address it wasn't given, so each further address is a
//! secondary private address of the interface (`secondary_private_ips` on an instance with one
//! interface, the ordered `private_ip_list` of a network interface), and a whole aligned /28 of
//! them is one IPv4 prefix (`ipv4_prefixes`), which takes one of the interface's slots. The
//! instance's own set-up puts them on the interface. An instance type has a number of addresses
//! per interface: a machine that needs more than its size gives gets a larger type.

use std::net::Ipv4Addr;

use isoloom_plan::MachinePlan;
use isoloom_render::hcl_builder::{BlockBuilder, boolean, list, string};

/// The further addresses of a machine on one network: those AWS takes one by one, and the /28
/// prefixes it takes whole.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Split {
    /// Each a secondary private address.
    pub single: Vec<Ipv4Addr>,
    /// Aligned /28 blocks every address of which is the machine's, as `a.b.c.d/28`.
    pub prefixes: Vec<String>,
}

/// The further addresses of `mp` on `network`, split (`prefixes`: whether whole /28s become
/// prefixes; a Windows instance has a single interface, which takes no prefix there).
pub fn split(mp: &MachinePlan, network: &str, prefixes: bool) -> Split {
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
pub fn needs_interfaces(mp: &MachinePlan) -> bool {
    mp.addresses.len() > 1 || mp.addresses.keys().any(|n| !split(mp, n, true).prefixes.is_empty())
}

/// An instance with one interface: its further addresses on its network, when it has some.
pub fn on_instance(instance: BlockBuilder, mp: &MachinePlan, network: &str) -> BlockBuilder {
    let s = split(mp, network, false);
    if s.single.is_empty() {
        return instance;
    }
    instance.add_attribute(("secondary_private_ips", list(s.single.iter().map(|a| string(a.to_string())))))
}

/// A network interface at `primary`, with the machine's further addresses on `network` after
/// it (in order, so the first stays the primary one) and its prefixes.
pub fn on_interface(nic: BlockBuilder, mp: Option<&MachinePlan>, network: &str, primary: &str) -> BlockBuilder {
    let s = mp.map(|mp| split(mp, network, true)).unwrap_or_default();
    if s.single.is_empty() && s.prefixes.is_empty() {
        return nic.add_attribute(("private_ips", list([string(primary)])));
    }
    let addresses = std::iter::once(primary.to_string()).chain(s.single.iter().map(ToString::to_string));
    let nic = nic
        .add_attribute(("private_ip_list_enabled", boolean(true)))
        .add_attribute(("private_ip_list", list(addresses.map(string))));
    if s.prefixes.is_empty() {
        return nic;
    }
    nic.add_attribute(("ipv4_prefixes", list(s.prefixes.iter().map(|p| string(p.as_str())))))
}

/// The addresses (prefixes counted once) each interface of `mp` needs: the most of any.
fn slots(mp: &MachinePlan, prefixes: bool) -> usize {
    mp.addresses
        .keys()
        .map(|n| {
            let s = split(mp, n, prefixes);
            1 + s.single.len() + s.prefixes.len()
        })
        .max()
        .unwrap_or(1)
}

/// The addresses per interface of the instance types Isoloom picks, smallest first: AWS's
/// "IP addresses per network interface" of each.
const PER_INTERFACE: &[(&str, usize)] = &[
    ("t3.micro", 2),
    ("t3.small", 4),
    ("c7i-flex.large", 10),
    ("m7i-flex.large", 10),
    ("m7i-flex.xlarge", 15),
];

/// `size`, or the smallest larger type whose interfaces take every address of `mp`.
pub fn sized(size: &'static str, mp: &MachinePlan) -> &'static str {
    let need = slots(mp, !mp.is_windows());
    let from = PER_INTERFACE.iter().position(|(t, _)| *t == size).unwrap_or(0);
    let fits = |(_, ips): &&(&str, usize)| *ips >= need;
    PER_INTERFACE[from..].iter().find(fits).map_or(size, |(t, _)| t)
}

/// Why AWS can't give machine `name` its addresses: more on one interface than its largest
/// type takes, or one of the first four of a subnet, which AWS keeps.
pub fn refusal(name: &str, mp: &MachinePlan) -> Option<String> {
    let most = PER_INTERFACE.last().map_or(0, |(_, ips)| *ips);
    if slots(mp, !mp.is_windows()) > most {
        return Some(format!(
            "machine `{name}`: AWS gives an interface at most {most} addresses here (a whole aligned /28 counts once)"
        ));
    }
    let kept = mp.more_addresses.iter().find(|(_, more)| more.iter().any(|a| a.octets()[3] <= 3));
    kept.map(|(net, _)| format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    fn plan(octets: &str) -> MachinePlan {
        let spec = isoloom_spec::parse(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: {octets} }}, vm: {{ os: debian-12, provision: [s.sh] }} }}\n"
        ))
        .unwrap();
        isoloom_plan::Environment::build(&spec, Target::CloudVm, None).machines["web"].clone()
    }

    #[test]
    fn a_whole_aligned_block_is_one_prefix() {
        let all: Vec<String> = (16..32).map(|o| o.to_string()).collect();
        let mp = plan(&format!("[10, 11, {}]", all.join(", ")));
        let s = split(&mp, "lan", true);
        assert_eq!(s.single, ["10.0.0.11".parse::<Ipv4Addr>().unwrap()]);
        assert_eq!(s.prefixes, ["10.0.0.16/28"]);
        assert!(needs_interfaces(&mp));
        assert_eq!(sized("t3.micro", &mp), "t3.small");
        assert_eq!(split(&mp, "lan", false).single.len(), 17);
    }

    #[test]
    fn a_larger_type_when_the_interface_needs_more_addresses() {
        let mp = plan("[10, 11, 12]");
        assert!(!needs_interfaces(&mp));
        assert_eq!(sized("t3.micro", &mp), "t3.small");
        assert_eq!(sized("m7i-flex.large", &mp), "m7i-flex.large");
        assert!(refusal("web", &mp).is_none());
        assert!(refusal("web", &plan("[10, 3]")).unwrap().contains("(.1 to .3)"));
        let many: Vec<String> = (10..30).map(|o| o.to_string()).collect();
        assert!(refusal("web", &plan(&format!("[{}]", many.join(", ")))).is_some());
    }
}
