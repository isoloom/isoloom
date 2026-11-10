//! The seal, on every target that seals: once its machines are set up, the run closes the
//! internet to the machines of every `internet: false` network at the network, where root in a
//! guest can't undo it. The machines had what they needed to set themselves up (packages,
//! images); from then on only the environment's own networks are reachable from them.
//!
//! What is sealed, and the switch's name, are the same everywhere; how differs. A cloud switches
//! its own rules with a module variable (`crate::cloud::vm::seal`); a server's router (Proxmox's,
//! ESXi's) applies a sealed ruleset it keeps from its first boot (`crate::server::router`), over
//! SSH (`crate::server::manifest`).

use isoloom_plan::{Environment, MachinePlan};

use crate::machine::vms;

/// The run's seal switch: the cloud modules' Terraform variable, and on a server the runner's
/// recorded flag.
pub(crate) const SEALED: &str = "isoloom_sealed";

/// Whether network `net` has no internet (`internet: false`).
pub(crate) fn closed(env: &Environment, net: &str) -> bool {
    env.networks.get(net).is_some_and(|p| !p.internet)
}

/// Whether machine `mp` is closed off once the run is sealed: every network it is on is an
/// `internet: false` one. Through a gateway machine (`gateway:`), it still reaches the
/// environment's other networks, never the internet.
pub(crate) fn sealed(env: &Environment, mp: &MachinePlan) -> bool {
    !mp.addresses.is_empty() && mp.addresses.keys().all(|n| closed(env, n))
}

/// Whether the run has anything to seal.
pub(crate) fn any(env: &Environment) -> bool {
    vms(env).any(|(_, mp)| sealed(env, mp))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    #[test]
    fn only_a_network_without_internet_seals_its_machines() {
        let env = crate::testing::planned(
            "version: 1\nname: s\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\n  back: { cidr: 10.61.20.0/24, internet: false }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n  cache: { os: debian@12, networks: { back: 20 } }\n",
            Target::Proxmox,
        );
        assert!(any(&env));
        let sealed_names: Vec<&str> = vms(&env).filter(|(_, mp)| sealed(&env, mp)).map(|(n, _)| n).collect();
        assert_eq!(sealed_names, ["cache"]);
        let open = crate::testing::planned(
            "version: 1\nname: o\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n",
            Target::Proxmox,
        );
        assert!(!any(&open));
    }
}
