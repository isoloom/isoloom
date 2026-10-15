//! A network's `mtu`, a machine's `rate_limit`, and `docker.lxc`: the link settings and the
//! Proxmox container a spec may ask for.

use super::Problems;
use super::networks::{RATES, measure, negative};
use crate::model::Spec;

/// The MTUs a target accepts: IPv4's smallest to the largest jumbo frame.
const MTU: std::ops::RangeInclusive<u16> = 576..=9216;

/// Each `mtu`, `rate_limit` and `docker.lxc`: well-formed, and `lxc` on a machine it applies to.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, net) in &spec.networks {
        if let Some(mtu) = net.mtu
            && !MTU.contains(&mtu)
        {
            p.add(format!("networks.{name}.mtu"), format!("{mtu} bytes is out of range: 576 to 9216"));
        }
    }
    for (name, m) in &spec.machines {
        if let Some(r) = &m.rate_limit {
            if negative(r) {
                p.add(format!("machines.{name}.rate_limit"), format!("`{r}`: a rate can't be negative"));
            } else if !measure(r, RATES) {
                p.add(format!("machines.{name}.rate_limit"), format!("`{r}` isn't a rate like 10mbit or 512kbit"));
            }
        }
        let Some(d) = m.docker.as_ref().filter(|d| d.lxc) else { continue };
        let at = format!("machines.{name}.docker.lxc");
        if d.image.is_none() {
            p.add(&at, "an LXC container runs a published image: `app.image` (not `build` or `firmware`)");
        }
        if d.appliance.is_some() {
            p.add(&at, "a network appliance is wired the way Docker attaches networks: drop `lxc`");
        }
        if m.vm.is_some() {
            p.add(&at, "the machine also has a VM form, which Proxmox runs instead: drop `lxc`");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    #[test]
    fn an_mtu_is_576_to_9216() {
        let mtu = |n: u32| {
            problems(&lab(&format!(
                "  big: {{ cidr: 10.1.0.0/24, mtu: {n} }}\nmachines:\n  web: {{ networks: {{ lan: 10, big: 10 }}, docker: {{ image: nginx }} }}\n"
            )))
        };
        assert_eq!((mtu(576), mtu(9216)), (Vec::<String>::new(), Vec::<String>::new()));
        assert_eq!(mtu(575), ["networks.big.mtu: 575 bytes is out of range: 576 to 9216"]);
        assert_eq!(mtu(9217), ["networks.big.mtu: 9217 bytes is out of range: 576 to 9216"]);
    }

    #[test]
    fn a_rate_limit_is_a_rate() {
        let rate = |r: &str| {
            problems(&lab(&format!(
                "machines:\n  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }}, rate_limit: {r} }}\n"
            )))
        };
        assert_eq!(rate("10mbit"), Vec::<String>::new());
        assert_eq!(rate("10mb"), ["machines.web.rate_limit: `10mb` isn't a rate like 10mbit or 512kbit"]);
    }

    #[test]
    fn an_lxc_container_runs_a_published_image_with_no_vm_form() {
        let lxc = |m: &str| problems(&lab(&format!("machines:\n  web: {{ networks: {{ lan: 10 }}, {m} }}\n")));
        assert_eq!(lxc("docker: { image: nginx, lxc: true }"), Vec::<String>::new());
        assert_eq!(
            lxc("docker: { build: web, lxc: true }"),
            ["machines.web.docker.lxc: an LXC container runs a published image: `app.image` (not `build` or `firmware`)"]
        );
        assert_eq!(
            lxc("docker: { image: x, appliance: cisco-iol, lxc: true }"),
            ["machines.web.docker.lxc: a network appliance is wired the way Docker attaches networks: drop `lxc`"]
        );
        assert_eq!(
            lxc("docker: { image: nginx, lxc: true }, vm: { os: debian@12 }"),
            ["machines.web.docker.lxc: the machine also has a VM form, which Proxmox runs instead: drop `lxc`"]
        );
    }
}
