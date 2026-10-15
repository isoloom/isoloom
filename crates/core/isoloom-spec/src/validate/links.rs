//! A network's `mtu`, a machine's `rate_limit`, and `docker.lxc`: the link settings and the
//! Proxmox container a spec may ask for.

use super::Problems;
use super::networks::{RATES, measure};
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
        if let Some(r) = &m.rate_limit
            && !measure(r, RATES)
        {
            p.add(format!("machines.{name}.rate_limit"), format!("`{r}` isn't a rate like 10mbit or 512kbit"));
        }
        let Some(d) = m.docker.as_ref().filter(|d| d.lxc) else { continue };
        let at = format!("machines.{name}.docker.lxc");
        if d.image.is_none() {
            p.add(&at, "an LXC container runs a published image: set `docker.image` (not `build` or `firmware`)");
        }
        if d.appliance.is_some() {
            p.add(&at, "a network appliance is wired the way Docker attaches networks: drop `lxc`");
        }
        if m.vm.is_some() {
            p.add(&at, "the machine has `vm:`, which Proxmox runs instead: drop `lxc`");
        }
    }
}
