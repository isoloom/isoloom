//! Published ports that only a format picking its own host ports can give: `publish: true` asks
//! for a free host port, so two machines (a counted machine's clones among them) may ask for one
//! number. Compose (Docker picks a free one) and Vagrant (`auto_correct`) take that; a target that
//! publishes every port at its number refuses it, once for the spec.

use std::collections::HashMap;
use std::fmt;

use isoloom_spec::model::{Protocol, Spec};

use crate::Requirement;

/// The first two machines asking for one host port when at least one of them asks for a free one
/// (`publish: true`); `None` when every host port is asked for once. Two numbers alike are the
/// spec's own problem (validation), so they never reach here.
pub(crate) fn requirement(spec: &Spec) -> Option<Requirement> {
    let mut asked: HashMap<(u16, Protocol), (&str, bool)> = HashMap::new();
    for (name, m) in &spec.machines {
        for p in &m.ports {
            let Some(host) = p.publish else { continue };
            match asked.get(&(host, p.protocol)) {
                Some(&(first, free)) if free || p.free => {
                    return Some(Requirement::SharedHostPort {
                        first: first.to_owned(),
                        second: name.clone(),
                        port: host,
                    });
                }
                Some(_) => {}
                None => {
                    asked.insert((host, p.protocol), (name, p.free));
                }
            }
        }
    }
    None
}

/// Why a target that publishes every port at its own number refuses two machines asking for one.
pub(crate) fn no_shared_host_port(f: &mut fmt::Formatter<'_>, first: &str, second: &str, port: u16) -> fmt::Result {
    write!(
        f,
        "machines `{first}` and `{second}` both publish on host port {port} (`publish: true` asks for a free one), and this target publishes every port at its number: give each its own `publish` number"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(machines: &str) -> Spec {
        let yaml = format!("version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n{machines}");
        crate::Lab::parse(&yaml).expect("the test spec parses").spec()
    }

    #[test]
    fn clones_of_a_free_port_share_its_number_and_a_number_alone_does_not() {
        let clones = spec("  web: { os: debian@12, count: 3, networks: { lab: 5 }, ports: { web: { port: 80, publish: true } } }\n");
        assert_eq!(
            requirement(&clones),
            Some(Requirement::SharedHostPort {
                first: "web-01".into(),
                second: "web-02".into(),
                port: 80
            })
        );
        let apart = spec(
            "  a: { os: debian@12, networks: { lab: 5 }, ports: { web: { port: 80, publish: true } } }\n  b: { os: debian@12, networks: { lab: 6 }, ports: { web: { port: 80, publish: 8080 } } }\n",
        );
        assert_eq!(requirement(&apart), None);
    }
}
