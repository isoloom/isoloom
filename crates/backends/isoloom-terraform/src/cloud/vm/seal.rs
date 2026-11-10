//! The seal on a cloud (`crate::seal`): a module variable, `isoloom_sealed`, switches each
//! cloud's own rules; the runner applies the module again with it at `true` once every machine
//! is ready (the run manifest's `seal`), and every later apply keeps it, so Terraform never sees
//! the seal as drift. Inbound is unchanged: the runner's SSH and the published ports still
//! answer from `allowed_cidr`, their replies let out by the clouds' stateful firewalls.
//!
//! Each cloud's resolver stays reachable (AWS's at the VPC's `.2`, Azure's `168.63.129.16`,
//! Google's metadata server, Oracle's VCN resolver): the clouds' firewalls don't filter them, so
//! a sealed machine can still resolve names, a channel out the seal leaves open there. On
//! DigitalOcean and Linode the resolvers are public addresses, which the seal closes.

use crate::hcl_builder::{Expression, conditional, list, reference, string};
use isoloom_plan::{Environment, MachinePlan};

use crate::module::variable::Variable;
use crate::seal::{SEALED, any, sealed};

/// The variable, when the run has anything to seal.
pub(crate) fn variable(env: &Environment) -> Option<Variable> {
    any(env).then(|| {
        Variable::new(SEALED, "bool")
            .default(crate::hcl_builder::boolean(false))
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
    fn the_variable_is_there_when_the_run_seals() {
        let env = crate::testing::planned(
            "version: 1\nname: s\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\n  back: { cidr: 10.61.20.0/24, internet: false }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n  cache: { os: debian@12, networks: { back: 20 } }\n",
            Target::CloudVm,
        );
        assert!(variable(&env).is_some());
        let open = crate::testing::planned(
            "version: 1\nname: o\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n",
            Target::CloudVm,
        );
        assert!(variable(&open).is_none());
    }
}
