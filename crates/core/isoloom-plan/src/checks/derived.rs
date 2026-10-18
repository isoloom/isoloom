//! Derived checks: what the spec itself promises. Every service answers from every machine that
//! `reach` lets through and not from the others, and offline networks don't reach the internet.

use super::{Host, HttpExpect, HttpRequest, Position, Probe, Resolved, TcpExchange, TcpExpect, Url, can_run_checks};
use isoloom_spec::model::{Machine, Port, Spec};

/// Whether `from` may open connections to `to`'s address on `network`, port `port`: they share
/// the network, or a `reach` rule from one of `from`'s networks opens it.
#[must_use]
pub fn allowed(spec: &Spec, from: &Machine, network: &str, port: u16) -> bool {
    from.networks.contains_key(network)
        || spec
            .reach
            .iter()
            .any(|r| from.networks.contains_key(&r.from) && r.to == network && (r.ports.is_empty() || r.ports.contains(&port)))
}

/// The checks the spec implies, from every machine that can run them:
/// - each TCP service of another machine answers at the addresses `reach` (or a shared network)
///   lets this machine through to (not a `dormant` one: it answers only once woken), and
/// - is blocked at every address when nothing lets it through at all. When one path is open and
///   another isn't, the closed one isn't asserted: a machine with several interfaces answers for
///   any of its addresses on an interface the traffic may use, so the outcome isn't the spec's
///   to promise;
/// - the internet doesn't answer from a machine whose networks are all offline.
///
/// When no machine can run checks (only Windows machines, or network appliances), they run from
/// the environment's networks instead (see [`from_networks`]).
pub fn derived(spec: &Spec) -> Vec<Resolved> {
    if !spec.machines.values().any(can_run_checks) {
        return from_networks(spec);
    }
    let mut out = Vec::new();
    for (a, ma) in &spec.machines {
        if !can_run_checks(ma) {
            continue;
        }
        let position = Position::Machine(a.clone());
        for (b, mb) in &spec.machines {
            if a == b {
                continue;
            }
            // A UDP service has no handshake: nothing tells "answers" from "blocked" without
            // speaking its protocol, so only the author's checks probe it.
            for svc in mb.ports.iter().filter(|s| s.protocol.is_tcp()) {
                out.extend(service_from(spec, &position, (a, ma), (b, mb), svc));
            }
        }
        if !ma.networks.keys().any(|n| spec.networks.get(n).is_some_and(|net| net.internet)) {
            let probe = Probe::Http {
                url: Url {
                    https: false,
                    host: Host::Literal("1.1.1.1".into()),
                    port: None,
                    path: "/".into(),
                },
                expect: HttpExpect::Blocked,
                request: HttpRequest::default(),
            };
            out.push(derived_check(format!("no internet from {a}"), position, probe, 0));
        }
    }
    out
}

/// The derived checks of machine `b`'s TCP service `svc` from machine `a`: it answers on every
/// path that is open, or is blocked on all of them when none is.
fn service_from(spec: &Spec, position: &Position, (a, ma): (&str, &Machine), (b, mb): (&str, &Machine), svc: &Port) -> Vec<Resolved> {
    let paths: Vec<(&String, bool)> = mb.networks.keys().map(|n| (n, allowed(spec, ma, n, svc.port))).collect();
    if paths.iter().all(|(_, ok)| !ok) {
        return paths
            .iter()
            .flat_map(|(n, _)| hosts(spec, b, mb, n))
            .map(|(host, at)| {
                let probe = Probe::Tcp {
                    host,
                    port: svc.port,
                    expect: TcpExpect::Blocked,
                    exchange: TcpExchange::default(),
                };
                derived_check(format!("{b}:{}{at} blocked from {a}", svc.port), position.clone(), probe, 0)
            })
            .collect();
    }
    // A dormant service answers once something wakes it, not before: nothing
    // to expect yet (still blocked from where `reach` doesn't let it through).
    if svc.dormant {
        return Vec::new();
    }
    paths
        .iter()
        .filter(|(_, ok)| *ok)
        .flat_map(|(n, _)| hosts(spec, b, mb, n))
        .map(|(host, at)| derived_check(format!("{b}:{}{at} from {a}", svc.port), position.clone(), answers(host, svc), 30))
        .collect()
}

/// Where machine `b` answers on network `n`, each with how a check's name says it: its address
/// there, then each of its further addresses (`lan: [10, 11]`) as written.
fn hosts(spec: &Spec, b: &str, mb: &Machine, n: &str) -> Vec<(Host, String)> {
    let primary = Host::Machine {
        name: b.to_owned(),
        network: n.to_owned(),
    };
    let more = crate::more_addresses::of(spec, mb).swap_remove(n).unwrap_or_default();
    std::iter::once((primary, on_network(mb, n)))
        .chain(more.into_iter().map(|a| (Host::Literal(a.to_string()), format!(" at {a}"))))
        .collect()
}

/// ` on <network>` when machine `m` is on several networks (the name must say which address).
fn on_network(m: &Machine, n: &str) -> String {
    if m.networks.len() > 1 { format!(" on {n}") } else { String::new() }
}

/// A check Isoloom derives from the spec.
fn derived_check(name: String, position: Position, probe: Probe, wait: u32) -> Resolved {
    Resolved {
        name,
        position,
        probe,
        wait,
        derived: true,
    }
}

/// The derived checks of an environment where no machine can run them (only Windows machines,
/// or network appliances): every service answers at each of its addresses, from the
/// environment's networks (on VM targets, the controller, which stands on every network).
/// Nothing stands where `reach` blocks a service or where a network is offline, so those aren't
/// asserted. A target where the machines that could run them don't (the containers of a hybrid
/// environment) uses it too.
#[must_use]
pub fn from_networks(spec: &Spec) -> Vec<Resolved> {
    let mut out = Vec::new();
    for (b, mb) in &spec.machines {
        // A dormant service answers only once woken, a UDP one isn't probed, as above.
        for svc in mb.ports.iter().filter(|s| !s.dormant && s.protocol.is_tcp()) {
            for (host, at) in mb.networks.keys().flat_map(|n| hosts(spec, b, mb, n)) {
                let name = format!("{b}:{}{at} from the networks", svc.port);
                out.push(derived_check(name, Position::Networks, answers(host, svc), 30));
            }
        }
    }
    out
}

/// The probe that a machine's service answers at `host`.
fn answers(host: Host, svc: &Port) -> Probe {
    if svc.http {
        Probe::Http {
            url: Url {
                https: svc.tls,
                host,
                port: Some(svc.port),
                path: "/".into(),
            },
            expect: HttpExpect::Any,
            request: HttpRequest::default(),
        }
    } else {
        Probe::Tcp {
            host,
            port: svc.port,
            expect: TcpExpect::Open,
            exchange: TcpExchange::default(),
        }
    }
}

#[cfg(test)]
mod tests;
