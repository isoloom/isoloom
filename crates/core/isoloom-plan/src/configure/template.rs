//! Filling in a template: its `{{ path }}` placeholders replaced by the lab's values for one
//! machine on one target (addresses move with a target's own network blocks), when Isoloom
//! generates. Logic-free, and never a run-time value or a secret: every CM tool then installs the
//! same bytes.

use isoloom_spec::model::Spec;
use isoloom_spec::syntax::placeholder::{self, Part, Place, PortField, Reference};

use crate::Environment;
use crate::addressing;
use crate::error::{Error, Result};

/// The values a template of machine `machine` can name, in the environment planned for a
/// target (its addresses on the target's own network blocks).
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    spec: &'a Spec,
    machine: &'a str,
}

impl<'a> Context<'a> {
    /// The values of machine `machine` of `env`.
    #[must_use]
    pub fn new(env: &'a Environment, machine: &'a str) -> Context<'a> {
        Context { spec: env.spec(), machine }
    }

    /// The value of `r`, when the spec has it.
    #[must_use]
    pub fn value(&self, r: &Reference) -> Option<String> {
        match r {
            Reference::MachineName => Some(self.machine.to_owned()),
            Reference::OwnAddress { network } => self.address(self.machine, network),
            Reference::Address { machine, network } => self.address(machine, network),
            Reference::Port {
                machine,
                port,
                field: PortField::Port,
            } => {
                let m = self.spec.machines.get(machine)?;
                let found = m.ports.iter().find(|p| p.name.as_deref() == Some(port.as_str()))?;
                Some(found.port.to_string())
            }
            _ => None,
        }
    }

    /// Machine `machine`'s address on `network`.
    fn address(&self, machine: &str, network: &str) -> Option<String> {
        let octet = self.spec.machines.get(machine)?.networks.get(network)?;
        Some(addressing::address(self.spec, network, *octet).to_string())
    }
}

/// `text` with its placeholders filled in from `context`.
///
/// # Errors
///
/// The first placeholder that can't be filled in: [`Error::Placeholder`] when it can't be read
/// in a template, [`Error::Unfilled`] when it names nothing the lab has. The spec's checks
/// report each one at validation, so a valid lab's templates always fill in.
pub fn fill(text: &str, context: &Context<'_>) -> Result<String> {
    let parts = placeholder::read(text, Place::Template).map_err(|wrong| {
        wrong
            .into_iter()
            .next()
            .map_or(Error::Placeholder(isoloom_spec::Error::Unclosed), Error::Placeholder)
    })?;
    let mut out = String::with_capacity(text.len());
    for part in parts {
        match part {
            Part::Text(t) => out.push_str(&t),
            Part::Placeholder(r) => out.push_str(&context.value(&r).ok_or(Error::Unfilled(r))?),
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::testing;

    const LAB: &str = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, command: x, ports: { site: 8080 } }\n  db: { os: debian@12, networks: { lab: 20 }, command: y, ports: { pg: 5432 } }\noverrides:\n  docker: { networks: { lab: { cidr: 10.70.0.0/24 } } }\n";

    #[test]
    fn a_template_is_filled_in_with_the_targets_values() {
        let text = "{{ machine.name }} {{ machine.addresses.lab }} -> {{ machines.db.addresses.lab }}:{{ machines.db.ports.pg.port }}\n";
        let vagrant = testing::environment(LAB, Target::Vagrant);
        assert_eq!(
            fill(text, &Context::new(&vagrant, "web")).ok().as_deref(),
            Some("web 10.9.0.10 -> 10.9.0.20:5432\n")
        );
        // Docker's own block moves the addresses.
        let docker = testing::environment(LAB, Target::Docker);
        assert_eq!(
            fill(text, &Context::new(&docker, "web")).ok().as_deref(),
            Some("web 10.70.0.10 -> 10.70.0.20:5432\n")
        );
    }

    #[test]
    fn a_placeholder_naming_nothing_or_a_run_time_value_is_reported() {
        let spec = testing::environment(LAB, Target::Vagrant);
        let wrong = fill(
            "{{ machines.db.ports.nope.port }} {{ machines.nope.addresses.lab }}",
            &Context::new(&spec, "web"),
        )
        .unwrap_err();
        assert_eq!(wrong.to_string(), "`{{ machines.db.ports.nope.port }}` names nothing this lab has");
        let run_time = fill("{{ machines.web.ports.site.published }}", &Context::new(&spec, "web")).unwrap_err();
        assert!(matches!(run_time, Error::Placeholder(isoloom_spec::Error::NotHere { .. })), "{run_time:?}");
        assert_eq!(fill("no placeholder {", &Context::new(&spec, "web")).ok().as_deref(), Some("no placeholder {"));
    }
}
