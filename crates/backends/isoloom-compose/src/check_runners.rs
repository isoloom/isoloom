//! The check runners: one per position, in the `check` profile, each a sh script next to the
//! Compose file. A machine's checks run in its network namespace (a stand-in's when the runner
//! supplies the access machine); checks without a position stand on every network; a machine's
//! `exec` checks run inside it (`docker compose exec`), from a script of their own.

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Host, Position, Probe, Render, Resolved};
use isoloom_render::container::{CHECK_IMAGE, UTILITY_IMAGE, exec_runner, init_names};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};

use crate::file::{Condition, NetworkMode, Service};
use crate::layout::{DIR, ROOT};
use crate::machine::Machine;
use crate::names::{STAND_IN, check_runner_service, routes_service};
use crate::{networks, sidecars};

/// The profile the runners (and the stand-in) are in: `up` leaves them out.
const PROFILE: &str = "check";

/// Adds each position's runner to `services`; returns their scripts.
pub(crate) fn services(env: &Environment, services: &mut IndexMap<String, Service>) -> Vec<GeneratedFile> {
    let spec = env.spec();
    let default_pos = checks::default_position(spec);
    let host = |h: &Host, _: &Position| -> String {
        match h {
            Host::Machine { name, network } => env.probe_address(name, network),
            Host::Literal(literal) => literal.clone(),
        }
    };
    let run_script = |path: &str| format!("cd /isoloom/project && sh {path}");
    let render = Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    let mut files = Vec::new();
    for (pos, group) in checks::by_position(spec, &env.checks) {
        let id = pos.id().to_owned();
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
        let runner = runner(env, &pos, scripts, services);
        services.insert(check_runner_service(&pos, &default_pos), runner);
    }
    files
}

/// One position's runner, after every machine answers (and its init jobs and routes are done).
fn runner(env: &Environment, pos: &Position, scripts: bool, services: &mut IndexMap<String, Service>) -> Service {
    let mut c = Service {
        image: Some(env.images.pull(CHECK_IMAGE)),
        profiles: vec![PROFILE.into()],
        entrypoint: vec!["/bin/sh".into(), "/isoloom/run.sh".into()],
        volumes: vec![format!("./checks/{}.sh:/isoloom/run.sh:ro", pos.id()).into()],
        ..Service::default()
    };
    if scripts {
        c.volumes.push(format!("{ROOT}:/isoloom/project:ro").into());
    }
    for m in Machine::all(env).filter(|m| m.spec.docker.is_some()) {
        if m.spec.has_readiness() {
            c.after(m.name, Condition::Healthy);
        }
        for init in init_names(m.name, m.spec) {
            c.after(init, Condition::CompletedSuccessfully);
        }
        if sidecars::needed(env, m) {
            c.after(routes_service(m.name), Condition::Healthy);
        }
    }
    match pos {
        Position::Machine(name) => {
            if let Some(m) = Machine::of(env, name) {
                let ns = namespace(env, m, &mut c, services);
                c.network_mode = Some(NetworkMode::Service(ns));
            }
        }
        Position::Networks => on_networks(env, &mut c),
    }
    c
}

/// The namespace a machine's checks run in: its own container, else the stand-in (added once,
/// with its sidecar, both only started with the check profile).
fn namespace(env: &Environment, m: Machine<'_>, c: &mut Service, services: &mut IndexMap<String, Service>) -> String {
    if m.spec.docker.is_some() {
        if !m.spec.has_readiness() {
            c.after(m.name, Condition::Started);
        }
        return m.name.to_owned();
    }
    let routed = sidecars::needed(env, m);
    if !services.contains_key(STAND_IN) {
        services.insert(STAND_IN.into(), stand_in(env, m));
        if routed {
            let mut sidecar = sidecars::routes(env, STAND_IN, m);
            sidecar.profiles = vec![PROFILE.into()];
            services.insert(routes_service(STAND_IN), sidecar);
        }
    }
    c.after(STAND_IN, Condition::Started);
    if routed {
        c.after(routes_service(STAND_IN), Condition::Healthy);
    }
    STAND_IN.to_owned()
}

/// An idle container at the access machine's addresses, for the check runner to stand in.
fn stand_in(env: &Environment, m: Machine<'_>) -> Service {
    Service {
        image: Some(env.images.pull(UTILITY_IMAGE)),
        hostname: Some(m.name.into()),
        networks: networks::attachments(env, m.plan),
        extra_hosts: networks::extra_hosts(env, m),
        entrypoint: vec!["sleep".into(), "infinity".into()],
        profiles: vec![PROFILE.into()],
        ..Service::default()
    }
}

/// A runner standing on every network. On offline networks that aren't Docker-internal, it
/// drops its default route like the machines do (root, to change its own routes).
fn on_networks(env: &Environment, c: &mut Service) {
    c.networks = env.networks.keys().map(|n| (n.clone(), None)).collect();
    let offline = !env.networks.values().any(|n| n.internet) && env.networks.keys().any(|n| !networks::internal(env, n));
    if offline {
        let drop = if env.networks.values().any(|n| n.ipv6.is_some()) {
            "ip route del default 2>/dev/null; ip -6 route del default 2>/dev/null; exec sh /isoloom/run.sh"
        } else {
            "ip route del default 2>/dev/null; exec sh /isoloom/run.sh"
        };
        c.user = Some("0".into());
        c.cap_add = vec!["NET_ADMIN".into()];
        c.entrypoint = vec!["/bin/sh".into(), "-c".into(), drop.into()];
    }
}
