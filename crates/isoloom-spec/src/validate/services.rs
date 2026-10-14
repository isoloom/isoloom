//! A machine's `services` (ports, protocols, published host ports) and what it waits for
//! (`depends_on`: machines with a service that answers).

use std::collections::{BTreeSet, HashMap};

use super::Problems;
use super::machines::Subject;
use crate::model::Protocol;

/// The machine's services: real ports listed once, HTTP and TLS on TCP, host ports published
/// by one machine only, and a `publish` port for `fixed`.
pub(super) fn of_machine(s: &Subject, published: &mut HashMap<(u16, Protocol), String>, p: &mut Problems) {
    let at = &s.at;
    let mut ports = BTreeSet::new();
    for (i, svc) in s.m.services.iter().enumerate() {
        if svc.port == 0 {
            p.add(format!("{at}.services[{i}].port"), "port 0 isn't a port");
        } else if !ports.insert((svc.port, svc.protocol)) {
            let udp = if svc.protocol.is_tcp() { "" } else { " (UDP)" };
            p.add(format!("{at}.services[{i}].port"), format!("port {}{udp} is listed twice", svc.port));
        }
        if !svc.protocol.is_tcp() && (svc.http || svc.tls) {
            p.add(format!("{at}.services[{i}]"), "`http` and `tls` run over TCP: a UDP service has neither");
        }
        match svc.publish {
            Some(0) => p.add(format!("{at}.services[{i}].publish"), "port 0 isn't a port"),
            Some(port) => {
                if let Some(other) = published.insert((port, svc.protocol), s.name.to_string()) {
                    p.add(format!("{at}.services[{i}].publish"), format!("port {port} is already published by `{other}`"));
                }
            }
            None if svc.fixed => p.add(format!("{at}.services[{i}].fixed"), "`fixed` keeps the `publish` port: give one"),
            None => {}
        }
    }
}

/// Each machine waited for exists, isn't this one, and has a service that answers a handshake.
pub(super) fn depends_on(s: &Subject, p: &mut Problems) {
    let spec = s.spec;
    for (i, dep) in s.m.depends_on.iter().enumerate() {
        let at = format!("{}.depends_on[{i}]", s.at);
        if dep == s.name {
            p.add(at, "a machine can't depend on itself");
        } else if !spec.machines.contains_key(dep) {
            p.add(at, format!("no machine named `{dep}`"));
        } else if spec.machines[dep].services.is_empty() {
            p.add(at, format!("`{dep}` has no services, so there's nothing to wait for; give it a service"));
        } else if spec.machines[dep].ready_ports().is_empty() {
            let dm = &spec.machines[dep];
            let why = if dm.services.iter().all(|s| s.dormant) {
                "are all dormant (they answer later)"
            } else if dm.services.iter().all(|s| !s.protocol.is_tcp()) {
                "are all UDP (no handshake to wait for)"
            } else {
                "are all dormant (they answer later) or UDP (no handshake to wait for)"
            };
            p.add(at, format!("`{dep}`'s services {why}, so there's nothing to wait for"));
        }
    }
}
