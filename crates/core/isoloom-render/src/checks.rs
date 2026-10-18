//! What the backends' check runners share: where a probe reaches its host.

use isoloom_plan::Environment;
use isoloom_plan::checks::Host;

/// A probe's host at its address: a machine where a probe reaches it on the network
/// ([`Environment::probe_address`]), a literal as written. For the targets that keep the
/// plan's addresses; one that hands addresses out itself (Kubernetes) reaches machines by name.
#[must_use]
pub fn address_of(env: &Environment, host: &Host) -> String {
    match host {
        Host::Machine { name, network } => env.probe_address(name, network),
        Host::Literal(literal) => literal.clone(),
    }
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_plan::checks::Host;
    use isoloom_spec::model::Target;

    #[test]
    fn a_machine_is_reached_at_its_address_and_a_literal_as_written() {
        let lab = isoloom_plan::Lab::parse(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\n",
        )
        .expect("parses");
        let env = Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None);
        let web = Host::Machine {
            name: "web".into(),
            network: "lan".into(),
        };
        assert_eq!(super::address_of(&env, &web), "10.9.0.10");
        assert_eq!(super::address_of(&env, &Host::Literal("example.com".into())), "example.com");
    }
}
