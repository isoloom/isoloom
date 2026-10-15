//! A machine's network sidecar: a container in its network namespace that sets what Docker
//! doesn't (its trunks, its routes through the router and gateways, its default route, none
//! when offline, `tc` on its interfaces), then stays so its healthcheck confirms them.

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
use isoloom_render::container::UTILITY_IMAGE;
use isoloom_render::{router, trunks};
use isoloom_spec::{validate, vlans};

use crate::file::{Condition, Healthcheck, NetworkMode, Restart, Seconds, Service};
use crate::images::TC_IMAGE;
use crate::interpolation::literal;
use crate::machine::Machine;
use crate::render::readiness;
use crate::{appliances, ipv6, networks};

/// Whether a machine gets a network sidecar, so dependents can wait for it.
pub(crate) fn needed(env: &Environment, m: Machine<'_>) -> bool {
    !commands(env, m).is_empty()
}

/// A container in `host`'s network namespace that sets the routes of machine `m`. It sets
/// them, then stays so the healthcheck can confirm them and `up --wait` treats it as running.
/// When the machine's container restarts, Docker gives it a new network namespace with the
/// default routes back; this sidecar, left in the old one with its addresses gone, exits then,
/// and its restart joins the new namespace and sets the routes again.
pub(crate) fn routes(env: &Environment, host: &str, m: Machine<'_>) -> Service {
    let spec = env.spec();
    let image = if impaired(env, m) { TC_IMAGE } else { UTILITY_IMAGE };
    // Its addresses gone means its namespace is (any family on an IPv6-only network).
    let family = if m.plan.addresses.keys().any(|n| ipv6::only(env, n)) { "" } else { "-4 " };
    let script = format!(
        "{} && while ip -o {family}addr show | grep -qv ' lo '; do sleep 2; done; exit 1",
        commands(env, m).join(" && ")
    );
    let mut r = Service {
        image: Some(env.images.pull(image)),
        network_mode: Some(NetworkMode::Service(host.to_owned())),
        cap_add: vec!["NET_ADMIN".into()],
        entrypoint: vec!["/bin/sh".into(), "-c".into(), script],
        healthcheck: Some(Healthcheck::shell(ready(env, m), Seconds(2), 30)),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    };
    r.after(host, Condition::Started);
    let ts = trunks::trunks(spec);
    for t in trunks::of(&ts, m.name) {
        match vlans::switch_of(spec, &t.lan) {
            // A switch appliance: started (IOS reports no readiness of its own; checks through
            // it `wait` for it to boot, as for any appliance on the way).
            Some(sw) => r.after(sw, Condition::Started),
            None => r.after(trunks::switch_name(&t.lan), Condition::Healthy),
        }
    }
    let through_router = m.spec.networks.keys().any(|n| env.networks.get(n).is_some_and(|p| p.gateway.is_none()));
    if env.router.is_some() && through_router {
        r.after(router::NAME, Condition::Healthy);
    }
    // The gateways it routes through, ready (answering, so their rules are loaded).
    for gw in validate::starts_after(spec, m.name, m.spec).into_iter().filter_map(|g| Machine::of(env, g)) {
        if m.spec.depends_on.iter().any(|d| d == gw.name) || !gw.plan.is_gateway || gw.spec.docker.is_none() {
            continue;
        }
        r.after(gw.name, readiness(gw.spec));
    }
    r
}

/// A gateway's way out: Docker's address on its first network with internet that the router
/// (or nothing) routes, so Docker doesn't pick one of the networks it serves.
fn own_default(env: &Environment, m: Machine<'_>) -> Option<Ipv4Addr> {
    if !m.plan.is_gateway || m.plan.default_gateway.is_some() {
        return None;
    }
    m.plan
        .addresses
        .keys()
        .filter_map(|n| env.networks.get(n))
        .find(|n| n.gateway.is_none() && n.internet)
        .map(|n| n.host)
}

/// Whether the offline machine has a network Docker doesn't already cut off.
fn drops_default(env: &Environment, m: Machine<'_>) -> bool {
    m.plan.offline && m.plan.addresses.keys().any(|n| !networks::internal(env, n))
}

/// Whether one of the machine's networks impairs its links with `tc`.
fn on_tc(env: &Environment, m: Machine<'_>) -> bool {
    m.plan.addresses.keys().any(|n| env.networks.get(n).is_some_and(|p| p.tc.is_some()))
}

/// The commands a machine's sidecar runs: its trunks first (the routes may go through addresses
/// only they carry), routes via the router and gateways, the default route through its gateway
/// (or a gateway's own way out), no default route when offline, then `tc` on its interfaces.
fn commands(env: &Environment, m: Machine<'_>) -> Vec<String> {
    let spec = env.spec();
    let address = |n: &str, o: u8| isoloom_render::address(spec, n, o);
    // An appliance routes itself (IOS): its sidecar only hands the data addresses over to it.
    if m.is_appliance() {
        return appliances::sidecar_commands(appliances::ports(spec, m.name, m.spec, &address).len());
    }
    let ts = trunks::trunks(spec);
    let mut cmds: Vec<String> = trunks::of(&ts, m.name)
        .flat_map(|t| trunks::machine_commands(spec, m.spec, t, address, |n| networks::docker_address(env, n)))
        .map(|c| literal(&c))
        .collect();
    cmds.extend(m.plan.routes.iter().map(|r| format!("ip route replace {} via {}", r.cidr, r.via)));
    cmds.extend(ipv6::route_commands(m.plan));
    if let Some(gw) = m.plan.default_gateway.or_else(|| own_default(env, m)) {
        cmds.push(format!("ip route replace default via {gw}"));
    }
    if drops_default(env, m) {
        cmds.push("(ip route del default 2>/dev/null || true)".into());
        cmds.extend(ipv6::on_ipv6(env, m.plan).then(|| "(ip -6 route del default 2>/dev/null || true)".into()));
    }
    // `tc` on its own interfaces, one subshell per line (`$$`: not Compose's interpolation).
    if let Some(script) = router::machine_tc_script(spec, m.spec, address) {
        cmds.extend(script.lines().map(|l| format!("({})", literal(l))));
    }
    cmds
}

/// What the sidecar's healthcheck confirms: its routes, default route, offline, `tc`, trunks.
fn ready(env: &Environment, m: Machine<'_>) -> String {
    // An appliance's sidecar only hands its data addresses over: ready once they're gone.
    if m.is_appliance() {
        return "! ip -o -4 addr show | grep -q ' eth[1-9]'".into();
    }
    let mut ready = Vec::new();
    if let Some(first) = m.plan.routes.first() {
        ready.push(format!("ip route show {} | grep -q via", first.cidr));
    }
    if let Some(first) = m.plan.routes6.first() {
        ready.push(format!("ip -6 route show {} | grep -q via", first.cidr));
    }
    if let Some(gw) = m.plan.default_gateway.or_else(|| own_default(env, m)) {
        ready.push(format!("ip route | grep -q '^default via {gw} '"));
    }
    if drops_default(env, m) {
        ready.push("! ip route | grep -q '^default'".into());
        ready.extend(ipv6::on_ipv6(env, m.plan).then(|| "! ip -6 route | grep -q '^default'".into()));
    }
    if on_tc(env, m) {
        ready.push("tc qdisc show | grep -q netem".into());
    }
    let ts = trunks::trunks(env.spec());
    ready.extend(trunks::of(&ts, m.name).map(trunks::machine_ready));
    ready.join(" && ")
}

/// Whether a machine's sidecar needs `tc` or 802.1Q (`ip link ... type vlan`): on a network
/// with `tc`, or on a trunk.
fn impaired(env: &Environment, m: Machine<'_>) -> bool {
    let vlans = m
        .plan
        .addresses
        .keys()
        .filter(|n| env.networks.get(*n).is_some_and(|p| p.vlan.is_some()))
        .count();
    on_tc(env, m) || m.plan.shape == isoloom_plan::MachineShape::Container && vlans >= 2
}
