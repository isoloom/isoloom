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
