//! Isoloom's router, when `reach` rules need one, and a switch per LAN with trunks (unless the
//! LAN names its own switch appliance): the services that route and bridge between the
//! environment's networks.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::container::UTILITY_IMAGE;
use isoloom_render::{router, trunks};
use isoloom_spec::vlans;

use crate::file::{Attachment, Healthcheck, Restart, Seconds, Service};
use crate::images::TC_IMAGE;
use crate::interpolation::literal;
use crate::ipv6;

/// The router's service when the environment has one, then each switch's.
pub(crate) fn services(env: &Environment, ts: &[trunks::Trunk], services: &mut IndexMap<String, Service>) {
    if let Some(addresses) = &env.router {
        services.insert(router::NAME.into(), router_service(env, addresses));
    }
    let mut lans: Vec<&str> = ts.iter().map(|t| t.lan.as_str()).collect();
    lans.dedup();
    lans.retain(|lan| vlans::switch_of(env.spec(), lan).is_none());
    for lan in lans {
        let of_lan: Vec<&trunks::Trunk> = ts.iter().filter(|t| t.lan == lan).collect();
        services.insert(trunks::switch_name(lan), switch_service(env, &of_lan));
    }
}

/// The router: on every network it routes at its last address, forwarding with the `reach`
/// rules (`$$` keeps Compose from interpolating the shell's variables).
fn router_service(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>) -> Service {
    let spec = env.spec();
    let mut environment = IndexMap::from([("RULES".to_owned(), router::nftables(spec))]);
    let tc = router::tc_script(spec);
    if let Some(t) = &tc {
        environment.insert("TC".into(), literal(t));
    }
    // With `tc`, iproute2 (for tc) and the netem commands after the rules.
    let start = if tc.is_some() {
        "apk add --no-cache nftables iproute2 >/dev/null && printf '%s' \"$$RULES\" | nft -f - && printf '%s' \"$$TC\" | sh && exec sleep infinity"
    } else {
        "apk add --no-cache nftables >/dev/null && printf '%s' \"$$RULES\" | nft -f - && exec sleep infinity"
    };
    Service {
        image: Some(env.images.pull(UTILITY_IMAGE)),
        hostname: Some(router::NAME.into()),
        cap_add: vec!["NET_ADMIN".into()],
        sysctls: ipv6::forwarding(env),
        networks: addresses.iter().map(|(n, a)| (n.clone(), Some(ipv6::router_attachment(env, n, *a)))).collect(),
        environment: Some(environment),
        entrypoint: vec!["/bin/sh".into(), "-c".into(), start.into()],
        healthcheck: Some(Healthcheck::shell("nft list table inet isoloom >/dev/null 2>&1", Seconds(3), 60)),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    }
}

/// A LAN's switch: on each of its VLAN networks and on each trunk to it, bridging them per VLAN.
fn switch_service(env: &Environment, ts: &[&trunks::Trunk]) -> Service {
    let spec = env.spec();
    let mut networks = IndexMap::new();
    for t in ts {
        for (net, _, _) in &t.vlans {
            networks.insert(net.clone(), Some(Attachment::at(trunks::switch_address(spec, net))));
        }
        networks.insert(trunks::link_network(t), Some(Attachment::with_mac(trunks::mac(t, 1))));
    }
    Service {
        image: Some(env.images.pull(TC_IMAGE)),
        cap_add: vec!["NET_ADMIN".into()],
        networks,
        entrypoint: vec!["/bin/sh".into(), "-c".into(), literal(&trunks::switch_script(spec, ts))],
        healthcheck: Some(Healthcheck::shell(trunks::switch_ready(ts), Seconds(2), 30)),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    }
}
