//! A network's `mtu` and a machine's `rate_limit`: what they ask of a backend, and why one
//! without them refuses. Which targets carry them is theirs to say, in their capabilities
//! (`isoloom matrix`).

use std::fmt;

use isoloom_spec::model::Spec;

use crate::requirements::Requirement;

/// The first network with an `mtu`, then the first machine with a `rate_limit`.
#[must_use]
pub(crate) fn requirements(spec: &Spec) -> Vec<Requirement> {
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
        "network `{network}` sets its `mtu`, which this target doesn't carry (`isoloom matrix` lists the targets that do)"
    )
}

/// Why a target refuses a machine's `rate_limit`.
pub(crate) fn no_rate_limit(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}` sets a `rate_limit`, which this target doesn't apply (`isoloom matrix` lists the targets that do)"
    )
}

#[cfg(test)]
mod tests {
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

    /// The refusals name no target: which ones carry the field is the targets' to say.
    #[test]
    fn a_refusal_names_no_other_target() {
        let mtu = crate::Requirement::NetworkMtu { network: "lan".into() }.to_string();
        assert_eq!(
            mtu,
            "network `lan` sets its `mtu`, which this target doesn't carry (`isoloom matrix` lists the targets that do)"
        );
        let rate = crate::Requirement::RateLimit { machine: "m".into() }.to_string();
        assert!(!rate.contains("proxmox") && rate.contains("`isoloom matrix`"), "{rate}");
    }
}
