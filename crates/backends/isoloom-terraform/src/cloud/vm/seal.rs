//! The seal: once its machines are set up, the run closes the internet to the machines of every
//! `internet: false` network at the network, where root in a guest can't undo it. The machines
//! had what they needed to set themselves up (packages, images); from then on only the lab's own
//! networks are reachable from them.
//!
//! A module variable, `isoloom_sealed`, switches each cloud's own rules; the runner applies the
//! module again with it at `true` once every machine is ready (the run manifest's `seal`), and
//! every later apply keeps it, so Terraform never sees the seal as drift. Inbound is unchanged:
//! the runner's SSH and the published ports still answer from `allowed_cidr`, their replies let
//! out by the clouds' stateful firewalls.
//!
//! Each cloud's resolver stays reachable (AWS's at the VPC's `.2`, Azure's `168.63.129.16`,
//! Google's metadata server, Oracle's VCN resolver): the clouds' firewalls don't filter them, so
//! a sealed machine can still resolve names, a channel out the seal leaves open there. On
//! DigitalOcean and Linode the resolvers are public addresses, which the seal closes.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Expression, conditional, list, reference, string};

use crate::machine::vms;
use crate::module::variable::Variable;

/// The module's variable that says the run is sealed.
pub(crate) const SEALED: &str = "isoloom_sealed";

/// Whether network `net` has no internet (`internet: false`).
pub(crate) fn closed(env: &Environment, net: &str) -> bool {
    env.networks.get(net).is_some_and(|p| !p.internet)
}

/// Whether machine `mp` is closed off once the run is sealed: every network it is on is an
/// `internet: false` one. Through a gateway machine (`gateway:`), it still reaches the lab's
/// other networks, never the internet.
pub(crate) fn sealed(env: &Environment, mp: &MachinePlan) -> bool {
    !mp.addresses.is_empty() && mp.addresses.keys().all(|n| closed(env, n))
}

/// Whether the run has anything to seal.
pub(crate) fn any(env: &Environment) -> bool {
    vms(env).any(|(_, mp)| sealed(env, mp))
}

/// The variable, when the run has anything to seal.
pub(crate) fn variable(env: &Environment) -> Option<Variable> {
    any(env).then(|| {
        Variable::new(SEALED, "bool")
            .default(isoloom_render::hcl_builder::boolean(false))
            .describe("Close the internet to the machines of every `internet: false` network, at the network: the runner sets it once the machines are set up")
    })
}

/// `var.isoloom_sealed ? <yes> : <no>`.
pub(crate) fn when_sealed(yes: Expression, no: Expression) -> Expression {
    conditional(reference(&format!("var.{SEALED}")), yes, no)
}

/// The lab's own blocks, every network's: where a sealed machine may still go.
pub(crate) fn lab_blocks(env: &Environment) -> Expression {
    list(env.networks.values().map(|p| string(p.cidr.as_str())))
}

/// `["0.0.0.0/0"]`, or the lab's blocks once sealed: where machine `mp` may go.
pub(crate) fn destinations(env: &Environment, mp: &MachinePlan) -> Expression {
    let anywhere = list([string("0.0.0.0/0")]);
    if sealed(env, mp) { when_sealed(lab_blocks(env), anywhere) } else { anywhere }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    #[test]
    fn only_a_network_without_internet_seals_its_machines() {
        let env = crate::testing::planned(
            "version: 1\nname: s\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\n  back: { cidr: 10.61.20.0/24, internet: false }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n  cache: { os: debian@12, networks: { back: 20 } }\n",
            Target::CloudVm,
        );
        assert!(any(&env) && variable(&env).is_some());
        let sealed_names: Vec<&str> = vms(&env).filter(|(_, mp)| sealed(&env, mp)).map(|(n, _)| n).collect();
        assert_eq!(sealed_names, ["cache"]);
        let open = crate::testing::planned(
            "version: 1\nname: o\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n",
            Target::CloudVm,
        );
        assert!(!any(&open) && variable(&open).is_none());
    }
}
