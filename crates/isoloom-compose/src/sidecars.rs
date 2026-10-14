//! The services Isoloom adds around the machines: its router, the VLAN switches, each machine's
//! network sidecar (routes, default route, offline, `tc`, trunks), the check runners and the
//! stand-in for an access machine the runner supplies.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::checks::{self, Position, Probe, Resolved};
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, router, trunks};
use isoloom_spec::{validate, vlans};

use crate::appliances;
use crate::docker::{CHECK_IMAGE, DIR, ROOT, STAND_IN, TC_IMAGE, UTILITY_IMAGE, exec_runner, init_names, ready_condition};
use crate::file::{Attachment, Healthcheck, Service};
use crate::render::mac;

/// Isoloom's router when `reach` rules need one, and a switch per LAN with trunks (see
/// `trunks`), unless the LAN names its own switch appliance.
pub(crate) fn router_and_switches(env: &Environment, ts: &[trunks::Trunk], services: &mut IndexMap<String, Service>) {
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
    let mut environment = IndexMap::from([("RULES".to_string(), router::nftables(spec))]);
    let tc = router::tc_script(spec);
    if let Some(t) = &tc {
        environment.insert("TC".into(), t.replace('$', "$$"));
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
        sysctls: crate::ipv6::forwarding(env),
        networks: addresses
            .iter()
            .map(|(n, a)| (n.clone(), Some(crate::ipv6::router_attachment(env, n, *a))))
            .collect(),
        environment: Some(environment),
        entrypoint: vec!["/bin/sh".into(), "-c".into(), start.into()],
        healthcheck: Some(Healthcheck::shell("nft list table inet isoloom >/dev/null 2>&1", "3s", 60)),
        restart: Some("unless-stopped".into()),
        ..Service::default()
    }
}

/// A LAN's switch: on each of its VLAN networks and on each trunk to it, bridging them per VLAN.
fn switch_service(env: &Environment, ts: &[&trunks::Trunk]) -> Service {
    let spec = env.spec();
    let mut networks = IndexMap::new();
    for t in ts {
        for (net, _, _) in &t.vlans {
            networks.insert(net.clone(), Attachment::at(trunks::switch_address(spec, net)));
        }
        networks.insert(trunks::link_network(t), mac(&trunks::mac(t, 1)));
    }
    Service {
        image: Some(env.images.pull(TC_IMAGE)),
        cap_add: vec!["NET_ADMIN".into()],
        networks,
        entrypoint: vec!["/bin/sh".into(), "-c".into(), trunks::switch_script(spec, ts).replace('$', "$$")],
        healthcheck: Some(Healthcheck::shell(trunks::switch_ready(ts), "2s", 30)),
        restart: Some("unless-stopped".into()),
        ..Service::default()
    }
}

/// A machine's networks, each at its address, with its aliases (resolved by Compose DNS).
pub(crate) fn attachments(env: &Environment, name: &str, mp: &MachinePlan) -> IndexMap<String, Option<Attachment>> {
    let aliases: Vec<String> = mp.names.iter().skip(1).cloned().collect();
    let _ = name;
    mp.addresses
        .iter()
        .map(|(n, a)| {
            let at = Attachment {
                aliases: aliases.clone(),
                ..crate::ipv6::attachment(env, mp, n, *a)
            };
            (n.clone(), Some(at))
        })
        .collect()
}

/// Names of machines that share no network with `name` (Compose DNS only resolves machines on
/// shared networks, and not across a network a trunk carries); they're reached through the
/// router. A LAN's switch has no address on the environment's networks: no name to resolve.
pub(crate) fn extra_hosts(env: &Environment, name: &str) -> Vec<String> {
    let spec = env.spec();
    let m = &spec.machines[name];
    let ts = trunks::trunks(spec);
    env.machines
        .iter()
        .filter(|(o, om)| {
            o.as_str() != name
                && !om.addresses.is_empty()
                && !om
                    .addresses
                    .keys()
                    .any(|n| m.networks.contains_key(n) && !trunks::carried(&ts, name, n) && !trunks::carried(&ts, o, n))
        })
        .flat_map(|(o, om)| {
            let a = env.address_for(name, o);
            om.names.iter().map(move |n| format!("{n}:{a}"))
        })
        .collect()
}

/// Whether a network is Docker's internal network: offline, nothing routes, and no machine on
/// it publishes a port (Docker can't publish from internal networks). With a router or a
/// gateway, Docker's internal-network firewall would drop forwarded traffic, so offline
/// machines lose their default route instead (in their sidecar).
pub(crate) fn internal(env: &Environment, network: &str) -> bool {
    let publishes = env
        .machines
        .values()
        .any(|m| m.shape == MachineShape::Container && m.addresses.contains_key(network) && !m.ports.is_empty());
    !env.networks[network].internet && !router::routed(env.spec()) && !publishes
}

/// Whether a machine gets a network sidecar, so dependents can wait for it.
pub(crate) fn needs_routes(env: &Environment, name: &str) -> bool {
    !route_commands(env, name).is_empty()
}

/// A gateway's way out: Docker's address on its first network with internet that the router
/// (or nothing) routes, so Docker doesn't pick one of the networks it serves.
fn own_default(env: &Environment, name: &str) -> Option<Ipv4Addr> {
    let mp = &env.machines[name];
    if !mp.is_gateway || mp.default_gateway.is_some() {
        return None;
    }
    mp.addresses
        .keys()
        .find(|n| env.networks[*n].gateway.is_none() && env.networks[*n].internet)
        .map(|n| env.networks[n].host)
}

/// Whether the offline machine has a network Docker doesn't already cut off.
fn drops_default(env: &Environment, name: &str) -> bool {
    let mp = &env.machines[name];
    mp.offline && mp.addresses.keys().any(|n| !internal(env, n))
}

/// The commands a machine's sidecar runs: its trunks first (the routes may go through addresses
/// only they carry), routes via the router and gateways, the default route through its gateway
/// (or a gateway's own way out), no default route when offline, then `tc` on its interfaces.
fn route_commands(env: &Environment, name: &str) -> Vec<String> {
    let spec = env.spec();
    let m = &spec.machines[name];
    let mp = &env.machines[name];
    let address = |n: &str, o: u8| isoloom_render::address(spec, n, o);
    // An appliance routes itself (IOS): its sidecar only hands the data addresses over to it.
    if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) {
        return appliances::sidecar_commands(appliances::ports(spec, name, m, &address).len());
    }
    let ts = trunks::trunks(spec);
    let mut cmds: Vec<String> = trunks::of(&ts, name)
        .flat_map(|t| trunks::machine_commands(spec, m, t, address, |n| env.networks[n].host))
        .map(|c| c.replace('$', "$$"))
        .collect();
    cmds.extend(mp.routes.iter().map(|r| format!("ip route replace {} via {}", r.cidr, r.via)));
    cmds.extend(crate::ipv6::route_commands(mp));
    if let Some(gw) = mp.default_gateway.or_else(|| own_default(env, name)) {
        cmds.push(format!("ip route replace default via {gw}"));
    }
    if drops_default(env, name) {
        cmds.push("(ip route del default 2>/dev/null || true)".into());
        cmds.extend(crate::ipv6::on_ipv6(env, mp).then(|| "(ip -6 route del default 2>/dev/null || true)".into()));
    }
    // `tc` on its own interfaces, one subshell per line (`$$`: not Compose's interpolation).
    if let Some(script) = router::machine_tc_script(spec, m, address) {
        cmds.extend(script.lines().map(|l| format!("({})", l.replace('$', "$$"))));
    }
    cmds
}

/// What the sidecar's healthcheck confirms: its routes, default route, offline, `tc`, trunks.
fn route_ready(env: &Environment, name: &str) -> String {
    let spec = env.spec();
    let m = &spec.machines[name];
    let mp = &env.machines[name];
    // An appliance's sidecar only hands its data addresses over: ready once they're gone.
    if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) {
        return "! ip -o -4 addr show | grep -q ' eth[1-9]'".into();
    }
    let mut ready = Vec::new();
    if let Some(first) = mp.routes.first() {
        ready.push(format!("ip route show {} | grep -q via", first.cidr));
    }
    if let Some(first) = mp.routes6.first() {
        ready.push(format!("ip -6 route show {} | grep -q via", first.cidr));
    }
    if let Some(gw) = mp.default_gateway.or_else(|| own_default(env, name)) {
        ready.push(format!("ip route | grep -q '^default via {gw} '"));
    }
    if drops_default(env, name) {
        ready.push("! ip route | grep -q '^default'".into());
        ready.extend(crate::ipv6::on_ipv6(env, mp).then(|| "! ip -6 route | grep -q '^default'".into()));
    }
    if mp.addresses.keys().any(|n| env.networks[n].tc.is_some()) {
        ready.push("tc qdisc show | grep -q netem".into());
    }
    let ts = trunks::trunks(spec);
    ready.extend(trunks::of(&ts, name).map(trunks::machine_ready));
    ready.join(" && ")
}

/// Whether a machine's sidecar needs `tc` or 802.1Q (`ip link ... type vlan`): on a network
/// with `tc`, or on a trunk.
fn impaired(env: &Environment, name: &str) -> bool {
    let mp = &env.machines[name];
    mp.addresses.keys().any(|n| env.networks[n].tc.is_some())
        || mp.shape == MachineShape::Container && mp.addresses.keys().filter(|n| env.networks[*n].vlan.is_some()).count() >= 2
}

/// A container in `host`'s network namespace that sets the routes of machine `name`. It sets
/// them, then stays so the healthcheck can confirm them and `up --wait` treats it as running.
/// When the machine's container restarts, Docker gives it a new network namespace with the
/// default routes back; this sidecar, left in the old one with its addresses gone, exits then,
/// and its restart joins the new namespace and sets the routes again.
pub(crate) fn routes(env: &Environment, host: &str, name: &str) -> Service {
    let spec = env.spec();
    let m = &spec.machines[name];
    let image = if impaired(env, name) { TC_IMAGE } else { UTILITY_IMAGE };
    // Its addresses gone means its namespace is (any family on an IPv6-only network).
    let family = if env.machines[name].addresses.keys().any(|n| crate::ipv6::only(env, n)) {
        ""
    } else {
        "-4 "
    };
    let script = format!(
        "{} && while ip -o {family}addr show | grep -qv ' lo '; do sleep 2; done; exit 1",
        route_commands(env, name).join(" && ")
    );
    let mut r = Service {
        image: Some(env.images.pull(image)),
        network_mode: Some(format!("service:{host}")),
        cap_add: vec!["NET_ADMIN".into()],
        entrypoint: vec!["/bin/sh".into(), "-c".into(), script],
        healthcheck: Some(Healthcheck::shell(route_ready(env, name), "2s", 30)),
        restart: Some("unless-stopped".into()),
        ..Service::default()
    };
    r.after(host, "service_started");
    let ts = trunks::trunks(spec);
    for t in trunks::of(&ts, name) {
        match vlans::switch_of(spec, &t.lan) {
            // A switch appliance: started (IOS reports no readiness of its own; checks through
            // it `wait` for it to boot, as for any appliance on the way).
            Some(sw) => r.after(sw, "service_started"),
            None => r.after(trunks::switch_name(&t.lan), "service_healthy"),
        }
    }
    if env.router.is_some() && m.networks.keys().any(|n| env.networks[n].gateway.is_none()) {
        r.after(router::NAME, "service_healthy");
    }
    // The gateways it routes through, ready (answering, so their rules are loaded).
    for gw in validate::starts_after(spec, name, m) {
        if m.depends_on.iter().any(|d| d == gw) || !env.machines[gw].is_gateway || spec.machines[gw].docker.is_none() {
            continue;
        }
        r.after(gw, ready_condition(&spec.machines[gw]));
    }
    r
}

/// An idle container at the access machine's addresses, for the check runner to stand in.
fn stand_in(env: &Environment, name: &str) -> Service {
    Service {
        image: Some(env.images.pull(UTILITY_IMAGE)),
        hostname: Some(name.into()),
        networks: attachments(env, name, &env.machines[name]),
        extra_hosts: extra_hosts(env, name),
        entrypoint: vec!["sleep".into(), "infinity".into()],
        profiles: vec!["check".into()],
        ..Service::default()
    }
}

/// The check runners: one per position, each a sh script next to the Compose file. A machine's
/// checks run in its network namespace (a stand-in's when the runner supplies the access
/// machine); checks without a position stand on every network. Returns the scripts.
pub(crate) fn check_runners(env: &Environment, services: &mut IndexMap<String, Service>) -> Vec<GeneratedFile> {
    let spec = env.spec();
    let default_pos = checks::default_position(spec);
    let host = |h: &checks::Host, _: &Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => env.probe_address(name, network),
        }
    };
    let run_script = |path: &str| format!("cd /isoloom/project && sh {path}");
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    let mut files = Vec::new();
    for (pos, group) in checks::by_position(spec, &env.checks) {
        let id = pos.id().to_string();
        let runner = if pos == default_pos {
            "isoloom-check".to_string()
        } else {
            format!("isoloom-check-{id}")
        };
        // `exec` checks run inside the machine itself, not in the runner beside it (which shares
        // only its network): their own script, piped to `docker compose exec -T <machine> sh -s`.
        let (execs, group): (Vec<&Resolved>, Vec<&Resolved>) = group.into_iter().partition(|c| matches!(c.probe, Probe::Exec { .. }));
        if !execs.is_empty() {
            files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/{}", exec_runner(&id)),
                contents: checks::script(&pos, &execs, &render),
            });
        }
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/{id}.sh"),
            contents: checks::script(&pos, &group, &render),
        });
        let scripts = group.iter().any(|c| matches!(c.probe, Probe::Script { .. }));
        let runner_svc = check_runner(env, &pos, scripts, services);
        services.insert(runner, runner_svc);
    }
    files
}

/// One position's runner, after every machine answers (and its init jobs and routes are done).
fn check_runner(env: &Environment, pos: &Position, scripts: bool, services: &mut IndexMap<String, Service>) -> Service {
    let spec = env.spec();
    let mut c = Service {
        image: Some(env.images.pull(CHECK_IMAGE)),
        profiles: vec!["check".into()],
        entrypoint: vec!["/bin/sh".into(), "/isoloom/run.sh".into()],
        volumes: vec![format!("./checks/{}.sh:/isoloom/run.sh:ro", pos.id())],
        ..Service::default()
    };
    if scripts {
        c.volumes.push(format!("{ROOT}:/isoloom/project:ro"));
    }
    for (name, m) in spec.machines.iter().filter(|(_, m)| m.docker.is_some()) {
        if m.has_readiness() {
            c.after(name.clone(), "service_healthy");
        }
        for init in init_names(name, m) {
            c.after(init, "service_completed_successfully");
        }
        if needs_routes(env, name) {
            c.after(format!("{name}-routes"), "service_healthy");
        }
    }
    match pos {
        Position::Machine(name) => {
            let ns = machine_namespace(env, name, &mut c, services);
            c.network_mode = Some(format!("service:{ns}"));
        }
        Position::Networks => on_networks(env, &mut c),
    }
    c
}

/// The namespace a machine's checks run in: its own container, else the stand-in (added once,
/// with its sidecar, both only started with the check profile).
fn machine_namespace(env: &Environment, name: &str, c: &mut Service, services: &mut IndexMap<String, Service>) -> String {
    let m = &env.spec().machines[name];
    if m.docker.is_some() {
        if !m.has_readiness() {
            c.after(name, "service_started");
        }
        return name.to_string();
    }
    if !services.contains_key(STAND_IN) {
        services.insert(STAND_IN.into(), stand_in(env, name));
        if needs_routes(env, name) {
            let mut sidecar = routes(env, STAND_IN, name);
            sidecar.profiles = vec!["check".into()];
            services.insert(format!("{STAND_IN}-routes"), sidecar);
        }
    }
    c.after(STAND_IN, "service_started");
    if needs_routes(env, name) {
        c.after(format!("{STAND_IN}-routes"), "service_healthy");
    }
    STAND_IN.to_string()
}

/// A runner standing on every network. On offline networks that aren't Docker-internal, it
/// drops its default route like the machines do (root, to change its own routes).
fn on_networks(env: &Environment, c: &mut Service) {
    c.networks = env.networks.keys().map(|n| (n.clone(), None)).collect();
    let offline = !env.networks.values().any(|n| n.internet) && env.networks.keys().any(|n| !internal(env, n));
    if offline {
        c.user = Some("0".into());
        c.cap_add = vec!["NET_ADMIN".into()];
        c.entrypoint = vec![
            "/bin/sh".into(),
            "-c".into(),
            if env.networks.values().any(|n| n.ipv6.is_some()) {
                "ip route del default 2>/dev/null; ip -6 route del default 2>/dev/null; exec sh /isoloom/run.sh".into()
            } else {
                "ip route del default 2>/dev/null; exec sh /isoloom/run.sh".into()
            },
        ];
    }
}
