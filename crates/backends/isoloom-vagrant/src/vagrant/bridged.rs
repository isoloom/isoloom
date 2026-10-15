//! Bridged networks (`bridged`): a VM joins the host's LAN as a Vagrant public network, after
//! its own networks, at a fixed address or by DHCP. Vagrant configures the interface in the
//! guest itself.

use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};
use isoloom_spec::validate::Cidr;

use crate::ruby::{Call, Ruby, Value};

/// The VM's public networks, one per bridged network it joins.
pub fn networks(r: &mut Ruby, env: &Environment, mp: &MachinePlan) {
    for (net, address) in &mp.outside {
        let Some(Join::Bridged(b)) = env.outside.get(net).map(|n| &n.join) else {
            continue;
        };
        let mut call = Call::new("m.vm.network").arg(Value::str("public_network"));
        match address {
            OutsideAddress::Fixed(ip) => call = call.kw("ip", Value::str(&ip.to_string())),
            OutsideAddress::Dhcp if b.default_route => call = call.kw("use_dhcp_assigned_default_route", true),
            OutsideAddress::Dhcp => {}
        }
        // Without one, Vagrant asks which interface when the host has several.
        if let Some(interface) = &b.interface {
            call = call.kw("bridge", Value::str(interface));
        }
        r.call(call);
    }
}

/// Why a VM can't have its bridged networks on Vagrant, if it can't: Vagrant's public network
/// takes no netmask (a fixed address is in a /24) and tags no VLAN, and a Windows guest's
/// addresses are set by Isoloom's own steps, which don't cover a bridged interface yet.
pub fn refusal(env: &Environment, name: &str, mp: &MachinePlan) -> Option<String> {
    for (net, address) in &mp.outside {
        let Some(n) = env.outside.get(net) else { continue };
        let Join::Bridged(b) = &n.join else { continue };
        if b.vlan.is_some() {
            return Some(format!("network `{net}`: Vagrant's bridged networks take no VLAN tag (Proxmox does)"));
        }
        if mp.is_windows() {
            return Some(format!("machine `{name}`: a bridged network on Windows comes later"));
        }
        let len = Cidr::parse(&n.cidr).map_or(0, |c| c.len);
        if matches!(address, OutsideAddress::Fixed(_)) && len != 24 {
            return Some(format!(
                "network `{net}`: a fixed address on a bridged network is in a /24 on Vagrant (its public networks take no netmask); use `dhcp: true`"
            ));
        }
    }
    None
}

/// Why the environment's bridged networks can't run under Vagrant's `provider`, if it has any:
/// QEMU links VMs through sockets, Hyper-V puts each network on a private switch of Isoloom's,
/// and libvirt bridges a device (`dev`) Isoloom doesn't name yet.
pub fn provider_refusal(env: &Environment, provider: &str) -> Option<String> {
    let (net, _) = env.outside.iter().find(|(_, n)| matches!(n.join, Join::Bridged(_)))?;
    Some(format!("network `{net}`: bridged networks under Vagrant's {provider} provider come later"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    fn env(home: &str) -> Environment {
        let yaml = format!(
            "version: 1\nname: t\nnetworks:\n  lan: {{ cidr: 10.20.0.0/24 }}\n  home: {home}\nmachines:\n  a: {{ networks: {{ lan: 10, home: 0 }}, vm: {{ os: debian@12 }} }}\n"
        );
        let spec = isoloom_spec::parse(&yaml).unwrap();
        Environment::build(&spec, Target::Vagrant, None)
    }

    fn rendered(env: &Environment) -> String {
        let mut r = Ruby::new();
        networks(&mut r, env, &env.machines["a"]);
        r.finish()
    }

    #[test]
    fn a_bridged_network_is_a_public_network() {
        let dhcp = env("{ cidr: 10.0.0.0/24, bridged: { interface: 'en0: Wi-Fi', dhcp: true, default_route: true } }");
        assert_eq!(
            rendered(&dhcp).trim(),
            r#"m.vm.network "public_network", use_dhcp_assigned_default_route: true, bridge: "en0: Wi-Fi""#
        );
        assert_eq!(refusal(&dhcp, "a", &dhcp.machines["a"]), None);
    }

    #[test]
    fn a_vlan_or_a_fixed_address_outside_a_24_is_refused() {
        let vlan = env("{ cidr: 10.0.0.0/24, bridged: { dhcp: true, vlan: 20 } }");
        assert!(refusal(&vlan, "a", &vlan.machines["a"]).unwrap().contains("VLAN"));
        let spec = isoloom_spec::parse(
            "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.20.0.0/24 }\n  home: { cidr: 10.0.0.0/26, bridged: {} }\nmachines:\n  a: { networks: { lan: 10, home: 20 }, vm: { os: debian@12 } }\n",
        )
        .unwrap();
        let fixed = Environment::build(&spec, Target::Vagrant, None);
        assert!(rendered(&fixed).contains(r#"ip: "10.0.0.20""#));
        assert!(refusal(&fixed, "a", &fixed.machines["a"]).unwrap().contains("/24"));
    }
}
