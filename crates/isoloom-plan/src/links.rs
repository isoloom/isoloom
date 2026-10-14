//! A network's `mtu` and a machine's `rate_limit`: what they ask of a backend, and why one
//! without them refuses. Only the proxmox target sets them (on its network devices) for now.

use std::fmt;

use isoloom_spec::model::Spec;

use crate::requirements::Requirement;

/// The first network with an `mtu`, then the first machine with a `rate_limit`.
pub fn requirements(spec: &Spec) -> Vec<Requirement> {
    let mtu = spec.networks.iter().find(|(_, n)| n.mtu.is_some());
    let rate = spec.machines.iter().find(|(_, m)| m.rate_limit.is_some());
    mtu.map(|(n, _)| Requirement::NetworkMtu { network: n.clone() })
        .into_iter()
        .chain(rate.map(|(m, _)| Requirement::RateLimit { machine: m.clone() }))
        .collect()
}

/// Why a target refuses a network's `mtu`.
pub(crate) fn no_mtu(f: &mut fmt::Formatter<'_>, network: &str) -> fmt::Result {
    write!(
        f,
        "network `{network}` sets its `mtu`, which this target doesn't carry yet (the proxmox target does)"
    )
}

/// Why a target refuses a machine's `rate_limit`.
pub(crate) fn no_rate_limit(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}` sets a `rate_limit`, which this target doesn't apply yet (the proxmox target does, on its network devices)"
    )
}

/// A `tc` rate (`10mbit`, `512kbit`, `2mbps`) in megabytes per second, as Proxmox's network
/// devices take a rate limit. `None` when it doesn't parse (validation rejects it first).
pub fn megabytes_per_second(rate: &str) -> Option<f64> {
    let digits = rate.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let value: f64 = digits.parse().ok()?;
    // tc: `bit` units count bits, `bps` units count bytes; k, m, g are powers of 1000.
    let bytes = match &rate[digits.len()..] {
        "bit" => value / 8.0,
        "kbit" => value * 1e3 / 8.0,
        "mbit" => value * 1e6 / 8.0,
        "gbit" => value * 1e9 / 8.0,
        "bps" => value,
        "kbps" => value * 1e3,
        "mbps" => value * 1e6,
        "gbps" => value * 1e9,
        _ => return None,
    };
    Some(bytes / 1e6)
}

#[cfg(test)]
mod tests {
    use super::megabytes_per_second;

    #[test]
    fn rates_become_megabytes_per_second() {
        assert_eq!(megabytes_per_second("10mbit"), Some(1.25));
        assert_eq!(megabytes_per_second("2mbps"), Some(2.0));
        assert_eq!(megabytes_per_second("512kbit"), Some(0.064));
        assert_eq!(megabytes_per_second("ten"), None);
    }
}
