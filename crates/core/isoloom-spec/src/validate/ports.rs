//! A machine's `ports` (ports, protocols, published host ports) and what it waits for
//! (`depends_on`: machines with a port that answers).

use std::collections::{BTreeSet, HashMap};

use super::Problems;
use super::machines::Subject;
use crate::model::Protocol;

/// The machine's ports: real ports listed once, HTTP and TLS on TCP, host ports published
/// by one machine only, and a `publish` port for `fixed`.
pub(super) fn of_machine(s: &Subject, published: &mut HashMap<(u16, Protocol), String>, p: &mut Problems) {
    let at = &s.at;
    let mut ports = BTreeSet::new();
    for (i, svc) in s.m.ports.iter().enumerate() {
        if svc.port == 0 {
            p.add(format!("{at}.ports[{i}].port"), "port 0 isn't a port");
        } else if !ports.insert((svc.port, svc.protocol)) {
            let udp = if svc.protocol.is_tcp() { "" } else { " (UDP)" };
            p.add(format!("{at}.ports[{i}].port"), format!("port {}{udp} is listed twice", svc.port));
        }
        if !svc.protocol.is_tcp() && (svc.http || svc.tls) {
            p.add(format!("{at}.ports[{i}]"), "`http` and `tls` run over TCP: a UDP port has neither");
        }
        match svc.publish {
            Some(0) => p.add(format!("{at}.ports[{i}].publish"), "port 0 isn't a port"),
            Some(port) => {
                if let Some(other) = published.insert((port, svc.protocol), s.name.to_owned()) {
                    p.add(format!("{at}.ports[{i}].publish"), format!("port {port} is already published by `{other}`"));
                }
            }
            None if svc.fixed => p.add(format!("{at}.ports[{i}].fixed"), "`fixed` keeps the `publish` port: give one"),
            None => {}
        }
    }
}

/// Machine `dep`, waited for, has a port that answers a handshake.
fn waited_for(dep: &str, waited: &crate::model::Machine, at: String, p: &mut Problems) {
    if waited.ports.is_empty() {
        p.add(at, format!("`{dep}` has no `ports`, so there's nothing to wait for; give it a port"));
    } else if waited.ready_ports().is_empty() {
        let why = if waited.ports.iter().all(|port| port.dormant) {
            "are all dormant (they answer later)"
        } else if waited.ports.iter().all(|port| !port.protocol.is_tcp()) {
            "are all UDP (no handshake to wait for)"
        } else {
            "are all dormant (they answer later) or UDP (no handshake to wait for)"
        };
        p.add(at, format!("`{dep}`'s ports {why}, so there's nothing to wait for"));
    }
}

/// Each machine waited for exists, isn't this one, and has a port that answers a handshake.
pub(super) fn depends_on(s: &Subject, p: &mut Problems) {
    let spec = s.spec;
    for (i, dep) in s.m.depends_on.iter().enumerate() {
        let at = format!("{}.depends_on[{i}]", s.at);
        if dep == s.name {
            p.add(at, "a machine can't depend on itself");
        } else {
            match spec.machines.get(dep) {
                None => p.add(at, format!("no machine named `{dep}`")),
                Some(waited) => waited_for(dep, waited, at, p),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of a web server whose `ports` are `ports`.
    fn ports(ports: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }}, ports: {ports} }}\n"
        )))
    }

    /// The problems of a client waiting for `depends_on`, beside a server with `ports`.
    fn waiting(depends_on: &str, ports: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  srv: {{ networks: {{ lan: 10 }}, docker: {{ image: x }}, ports: {ports} }}\n  cli: {{ networks: {{ lan: 11 }}, docker: {{ image: x }}, depends_on: {depends_on} }}\n"
        )))
    }

    #[test]
    fn a_port_is_listed_once_per_protocol() {
        assert_eq!(ports("[{ port: 53 }, { port: 53, protocol: udp }]"), Vec::<String>::new());
        assert_eq!(ports("[{ port: 0 }]"), ["machines.web.ports[0].port: port 0 isn't a port"]);
        assert_eq!(ports("[{ port: 80 }, { port: 80 }]"), ["machines.web.ports[1].port: port 80 is listed twice"]);
        assert_eq!(
            ports("[{ port: 53, protocol: udp }, { port: 53, protocol: udp }]"),
            ["machines.web.ports[1].port: port 53 (UDP) is listed twice"]
        );
    }

    #[test]
    fn http_and_tls_run_over_tcp() {
        assert_eq!(ports("[{ port: 443, http: true, tls: true }]"), Vec::<String>::new());
        let udp = "machines.web.ports[0]: `http` and `tls` run over TCP: a UDP port has neither";
        assert_eq!(ports("[{ port: 443, protocol: udp, http: true }]"), [udp]);
        assert_eq!(ports("[{ port: 443, protocol: udp, tls: true }]"), [udp]);
    }

    #[test]
    fn a_fixed_port_keeps_its_published_port() {
        assert_eq!(ports("[{ port: 80, publish: 8080, fixed: true }]"), Vec::<String>::new());
        assert_eq!(
            ports("[{ port: 80, fixed: true }]"),
            ["machines.web.ports[0].fixed: `fixed` keeps the `publish` port: give one"]
        );
    }

    #[test]
    fn a_machine_waits_for_another_machines_answering_port() {
        assert_eq!(waiting("[srv]", "[{ port: 80 }, { port: 53, protocol: udp }]"), Vec::<String>::new());
        assert_eq!(
            waiting("[cli]", "[{ port: 80 }]"),
            [
                "machines.cli.depends_on[0]: a machine can't depend on itself",
                "machines: depends_on forms a cycle: cli -> cli"
            ]
        );
        assert_eq!(waiting("[db]", "[{ port: 80 }]"), ["machines.cli.depends_on[0]: no machine named `db`"]);
        assert_eq!(
            waiting("[srv]", "[]"),
            ["machines.cli.depends_on[0]: `srv` has no `ports`, so there's nothing to wait for; give it a port"]
        );
    }

    #[test]
    fn a_wait_needs_a_port_that_answers_a_handshake_now() {
        let nothing = |why: &str| [format!("machines.cli.depends_on[0]: `srv`'s ports {why}, so there's nothing to wait for")];
        assert_eq!(
            waiting("[srv]", "[{ port: 80, dormant: true }]"),
            nothing("are all dormant (they answer later)")
        );
        assert_eq!(
            waiting("[srv]", "[{ port: 53, protocol: udp }]"),
            nothing("are all UDP (no handshake to wait for)")
        );
        assert_eq!(
            waiting("[srv]", "[{ port: 80, dormant: true }, { port: 53, protocol: udp }]"),
            nothing("are all dormant (they answer later) or UDP (no handshake to wait for)")
        );
    }
}
