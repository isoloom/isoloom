//! `networks`: blocks (valid, private, /24 to /29, not overlapping), gateways, a LAN's switch,
//! the appliances' management network, link impairment, and the blocks on Docker.

use crate::contract::KEBAB_MAX;

use super::{Blocks, Problems, block, dns_label};
use crate::model::{Network, Spec, Tc};
use crate::net::{Cidr, DOCKER_BLOCK, NetworkBlock, PRIVATE};

/// At least one network.
pub(super) fn declared(spec: &Spec, problems: &mut Problems) {
    if spec.networks.is_empty() {
        problems.add("networks", "declare at least one network");
    }
}

/// Every problem of the LANs split into VLANs. Lowering refuses on the first of them and leaves
/// the VLANs unflattened, so the spec validated here still has them and the author sees them all.
pub(super) fn vlans(spec: &Spec, p: &mut Problems) {
    for found in crate::vlans::problems(spec) {
        p.0.push(super::Problem::new(found.code, found.path.to_string(), found.reason));
    }
}

/// Each network's name and block; returns the blocks that parse, in order.
pub(super) fn blocks(spec: &Spec, problems: &mut Problems) -> Blocks {
    let mut blocks: Blocks = Vec::new();
    for (name, net) in &spec.networks {
        let at = format!("networks.{name}");
        if name.len() > KEBAB_MAX {
            problems.add(&at, format!("network names are at most {KEBAB_MAX} characters: a DNS label"));
        } else if !dns_label(name) {
            problems.add(&at, "network names are kebab-case and start with a letter");
        }
        let Some(c) = Cidr::parse(&net.cidr) else {
            problems.add(format!("{at}.cidr"), format!("`{}` isn't a network address like 10.20.0.0/24", net.cidr));
            continue;
        };
        if !PRIVATE.iter().any(|r| r.contains(c)) {
            problems.add(format!("{at}.cidr"), "use a private block: inside 10.0.0.0/8, 172.16.0.0/12 or 192.168.0.0/16");
        } else if c.prefix() < 24 || c.prefix() > 29 {
            problems.add(format!("{at}.cidr"), "use a /24 to /29 block");
        }
        for (other, oc) in &blocks {
            if c.overlaps(*oc) {
                problems.add(format!("{at}.cidr"), format!("overlaps network `{other}`"));
            }
        }
        docker_block(&at, net, c, problems);
        blocks.push((name.clone(), c));
    }
    blocks
}

/// A network's own `docker.cidr`: inside 10.0.0.0/8 and the size of its `cidr`.
fn docker_block(at: &str, net: &Network, c: Cidr, problems: &mut Problems) {
    let Some(d) = &net.docker else { return };
    match Cidr::parse(&d.cidr) {
        Some(dc) if dc.prefix() == c.prefix() && DOCKER_BLOCK.contains(dc) => {}
        Some(_) => problems.add(
            format!("{at}.docker.cidr"),
            format!(
                "use a /{} block inside 10.0.0.0/8 (the same size as `cidr`, so addresses keep their last octet)",
                c.prefix()
            ),
        ),
        None => problems.add(format!("{at}.docker.cidr"), format!("`{}` isn't a network address like 10.20.0.0/24", d.cidr)),
    }
}

/// A network's `gateway`: a machine on it (at the gateway address, checked with the machines).
pub(super) fn gateways(spec: &Spec, blocks: &Blocks, problems: &mut Problems) {
    for (name, net) in &spec.networks {
        let Some(gw) = &net.gateway else { continue };
        let at = format!("networks.{name}.gateway");
        match spec.machines.get(gw) {
            None => problems.unknown(at, format!("no machine named `{gw}`")),
            Some(m) if !m.networks.contains_key(name) => {
                let octet = block(blocks, name).and_then(NetworkBlock::new).map_or(1, NetworkBlock::gateway_octet);
                problems.add(
                    at,
                    format!("attach `{gw}` to `{name}` at the gateway address: `networks: {{ {name}: {octet} }}`"),
                );
            }
            Some(_) => {}
        }
    }
}

/// A LAN's `switch`: a switch appliance whose ports the LAN gives it.
pub(super) fn switches(spec: &Spec, problems: &mut Problems) {
    let mut switched: Vec<(&str, &str)> = Vec::new();
    for (net, n) in &spec.networks {
        let Some(sw) = n.switch.as_deref() else { continue };
        let Some((lan, _)) = &n.vlan else {
            let message = if crate::vlans::of_lan(spec, net).next().is_some() {
                format!("machines join `{net}` directly (its untagged part), which the switch doesn't carry: put them on one of its VLANs")
            } else {
                "only a LAN split into `vlans` has a switch".to_owned()
            };
            problems.add(format!("networks.{net}.switch"), message);
            continue;
        };
        if switched.iter().any(|(l, _)| l == lan) {
            continue;
        }
        // A switch that doesn't exist is reported for each of the LAN's VLANs naming it.
        if switch(spec, lan, sw, &switched, problems) {
            switched.push((lan, sw));
        }
    }
}

/// The machine `sw` switching LAN `lan`: a switch appliance, of one LAN, with no address of its
/// own. False when there is no such machine.
fn switch(spec: &Spec, lan: &str, sw: &str, switched: &[(&str, &str)], problems: &mut Problems) -> bool {
    let at = format!("networks.{lan}.switch");
    let Some(m) = spec.machines.get(sw) else {
        problems.unknown(at, format!("no machine named `{sw}`"));
        return false;
    };
    if !m.docker.as_ref().and_then(|d| d.appliance).is_some_and(crate::appliance::is_switch) {
        problems.add(&at, format!("`{sw}` isn't a switch: give it `app.appliance: cisco-iol-l2` or `cisco-vios-l2`"));
    }
    if let Some((other, _)) = switched.iter().find(|(_, s)| *s == sw) {
        problems.add(&at, format!("`{sw}` already switches LAN `{other}`: a switch serves one LAN"));
    }
    if !m.networks.is_empty() {
        problems.add(
            format!("machines.{sw}.networks"),
            format!(
                "`{sw}` switches LAN `{lan}`: its ports come from the LAN (an access port per VLAN, a trunk per machine on several of its VLANs); leave `networks` out"
            ),
        );
    }
    if !m.ports.is_empty() || !m.aliases.is_empty() {
        problems.add(
            format!("machines.{sw}"),
            format!("`{sw}` switches LAN `{lan}` and has no address on the environment's networks: no `ports` or `aliases`"),
        );
    }
    true
}

/// With network appliances, no network may overlap their management network.
pub(super) fn management(spec: &Spec, problems: &mut Problems) {
    let any_appliance = spec.machines.values().any(|m| m.docker.as_ref().is_some_and(|d| d.appliance.is_some()));
    if !any_appliance {
        return;
    }
    let Some(mgmt) = Cidr::parse(crate::appliance::MGMT_CIDR) else { return };
    for (net, n) in &spec.networks {
        if Cidr::parse(&n.cidr).is_some_and(|c| c.overlaps(mgmt)) {
            problems.add(
                format!("networks.{net}.cidr"),
                format!("overlaps {}, the network appliances' management network", crate::appliance::MGMT_CIDR),
            );
        }
    }
}

/// Link impairment: well-formed. It applies on the machines' interfaces and the router's.
pub(super) fn tc(spec: &Spec, problems: &mut Problems) {
    for (name, net) in &spec.networks {
        let Some(tc) = &net.tc else { continue };
        let at = format!("networks.{name}.tc");
        if tc.delay.is_none() && tc.jitter.is_none() && tc.loss.is_none() && tc.rate.is_none() {
            problems.add(&at, "say what to impair: `delay`, `jitter`, `loss` or `rate`");
        }
        tc_values(&at, tc, problems);
        if net.gateway.is_some() {
            problems.add(
                &at,
                format!("`{name}` is routed by its gateway machine, which owns its link; `tc` applies on Isoloom's router"),
            );
        }
    }
}

/// A number followed by one of `units`, like `50ms` or `1.5s`.
pub(super) fn measure(v: &str, units: &[&str]) -> bool {
    let digits = v.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = v.strip_prefix(digits).unwrap_or_default();
    !digits.is_empty() && digits.parse::<f64>().is_ok() && units.contains(&unit)
}

/// A rate, as [`Rate`](crate::net::Rate) reads it: why it isn't one, at `at`.
pub(super) fn rate(at: &str, v: &str, problems: &mut Problems) {
    match crate::net::Rate::parse(v) {
        Ok(_) => {}
        Err(crate::net::RateError::Negative) => problems.add(at, format!("`{v}`: a rate can't be negative")),
        Err(crate::net::RateError::Malformed) => problems.add(at, format!("`{v}` isn't a rate like 10mbit or 512kbit")),
    }
}

/// Whether `v` is a measure below zero (`-5ms`), which netem refuses when it runs.
pub(super) fn negative(v: &str) -> bool {
    v.trim_start().starts_with('-')
}

/// Each impairment value: times, a percentage, a rate; none below zero.
fn tc_values(at: &str, tc: &Tc, problems: &mut Problems) {
    const TIMES: &[&str] = &["ms", "s", "us"];
    for (field, value) in [("delay", &tc.delay), ("jitter", &tc.jitter)] {
        if let Some(v) = value
            && negative(v)
        {
            problems.add(format!("{at}.{field}"), format!("`{v}`: a {field} can't be negative"));
        }
    }
    if let Some(d) = &tc.delay
        && !negative(d)
        && !measure(d, TIMES)
    {
        problems.add(format!("{at}.delay"), format!("`{d}` isn't a time like 50ms or 1s"));
    }
    if let Some(j) = &tc.jitter {
        if !negative(j) && !measure(j, TIMES) {
            problems.add(format!("{at}.jitter"), format!("`{j}` isn't a time like 5ms"));
        }
        if tc.delay.is_none() {
            problems.add(format!("{at}.jitter"), "jitter varies a `delay`; set one");
        }
    }
    if let Some(l) = tc.loss
        && !(0.0..=100.0).contains(&l)
    {
        problems.add(format!("{at}.loss"), "a percentage, 0 to 100");
    }
    if let Some(r) = &tc.rate {
        rate(&format!("{at}.rate"), r, problems);
    }
}

/// On Docker, the networks (moved or not) mustn't overlap either. Valid blocks only.
pub(super) fn docker_overlaps(spec: &Spec, problems: &mut Problems) {
    let docker = crate::net::docker_cidrs(spec);
    for (i, (a, ca)) in docker.iter().enumerate() {
        for (b, cb) in docker.iter().skip(i + 1) {
            if ca.overlaps(*cb) {
                problems.add(
                    format!("networks.{b}.docker.cidr"),
                    format!("on Docker, overlaps network `{a}`; choose another block"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
