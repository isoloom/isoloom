//! A network's `mtu` and a machine's `rate_limit`: what they ask of a backend, and why one
//! without them refuses. Only the proxmox target sets them (on its network devices) for now.

use std::fmt;

use isoloom_spec::model::Spec;

use crate::requirements::Requirement;

/// The first network with an `mtu`, then the first machine with a `rate_limit`.
#[must_use]
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
#[must_use]
pub fn megabytes_per_second(rate: &str) -> Option<f64> {
    let digits = rate.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let value: f64 = digits.parse().ok()?;
    // tc: `bit` units count bits, `bps` units count bytes; k, m, g are powers of 1000.
    let bytes = match rate.strip_prefix(digits)? {
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
    use proptest::prelude::*;

    use super::megabytes_per_second;

    /// Each unit and the bytes per second one of it is.
    const UNITS: [(&str, f64); 8] = [
        ("bit", 0.125),
        ("kbit", 125.0),
        ("mbit", 125_000.0),
        ("gbit", 125_000_000.0),
        ("bps", 1.0),
        ("kbps", 1e3),
        ("mbps", 1e6),
        ("gbps", 1e9),
    ];

    #[test]
    fn a_networks_mtu_and_a_machines_rate_limit_are_asked_for_first_found_each() {
        let spec = crate::testing::spec(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/24, mtu: 1400 }, c: { cidr: 10.3.0.0/24, mtu: 9000 } }\nmachines:\n  m: { networks: { a: 10 }, app: { image: x } }\n  n: { networks: { b: 10 }, app: { image: x }, rate_limit: 10mbit }\n",
        );
        assert_eq!(
            super::requirements(&spec),
            [
                crate::Requirement::NetworkMtu { network: "b".into() },
                crate::Requirement::RateLimit { machine: "n".into() }
            ]
        );
        assert_eq!(super::requirements(&crate::testing::spec("version: 1\nname: t\nmachines: {}\n")), []);
    }

    proptest! {
        #[test]
        fn a_rate_is_its_value_times_its_unit(value in 0u32..1_000_000, unit in 0..UNITS.len()) {
            let (name, bytes) = UNITS[unit];
            let got = megabytes_per_second(&format!("{value}{name}"));
            let want = f64::from(value) * bytes / 1e6;
            prop_assert!(got.is_some_and(|g| (g - want).abs() <= want * 1e-12), "{got:?} for {value}{name}");
        }

        #[test]
        fn a_rate_without_a_known_unit_is_none(value in 0u32..1000, unit in "[a-z]{0,6}") {
            prop_assume!(!UNITS.iter().any(|(u, _)| *u == unit));
            prop_assert_eq!(megabytes_per_second(&format!("{value}{unit}")), None);
        }

        #[test]
        fn any_text_reads_without_a_panic(rate in "\\PC{0,20}") {
            let _rate = megabytes_per_second(&rate);
        }
    }

    #[test]
    fn rates_become_megabytes_per_second() {
        assert_eq!(megabytes_per_second("10mbit"), Some(1.25));
        assert_eq!(megabytes_per_second("2mbps"), Some(2.0));
        assert_eq!(megabytes_per_second("512kbit"), Some(0.064));
        assert_eq!(megabytes_per_second("ten"), None);
    }
}
