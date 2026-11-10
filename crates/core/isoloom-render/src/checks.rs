//! What the backends' check runners share: where a probe reaches its host, and how a VM target
//! runs a project check script.

use isoloom_plan::Environment;
use isoloom_plan::checks::Host;

use crate::steps::PROJECT_DIR;

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

/// The shell command a VM target runs a project check script with: from the project copy at
/// [`PROJECT_DIR`], then `sh <path>` (what a backend hands [`isoloom_plan::checks::Render::script`]).
#[must_use]
pub fn script_command(path: &str) -> String {
    script_command_in(PROJECT_DIR, path)
}

/// The shell command running project check script `path` from the project copy at `project`,
/// the path quoted (a project path may hold a space).
#[must_use]
pub fn script_command_in(project: &str, path: &str) -> String {
    format!("cd {project} && sh {}", isoloom_spec::shell::quote(path))
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
