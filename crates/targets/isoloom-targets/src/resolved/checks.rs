//! The snapshot's checks, by position, with the runner each target names them by.

use isoloom_plan::addressing::address;
use isoloom_plan::checks::{self, Host, HttpExpect, Position, Probe, Resolved, TcpExpect};
use isoloom_spec::model::Spec;
use serde_json::{Value, json};

/// The checks section: the default position, then each position's runner and checks.
pub(super) fn section(spec: &Spec) -> Value {
    let plan = checks::plan(spec);
    let default_pos = checks::default_position(spec);
    let positions: Vec<Value> = checks::by_position(spec, &plan)
        .into_iter()
        .map(|(pos, group)| {
            let runner = if pos == default_pos {
                "isoloom-check".to_owned()
            } else {
                format!("isoloom-check-{}", pos.id())
            };
            json!({
                "position": pos.id(),
                "machine": match &pos { Position::Machine(m) => Some(m.clone()), Position::Networks => None },
                "runner": runner,
                "checks": group.iter().map(|c| check(spec, c)).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "default_position": default_pos.id(),
        "positions": positions,
    })
}

/// One check: its name, kind, target and expectation.
fn check(spec: &Spec, c: &Resolved) -> Value {
    let (kind, target, expect) = probe_parts(spec, &c.probe);
    let mut v = serde_json::Map::new();
    v.insert("name".into(), json!(c.name));
    v.insert("derived".into(), json!(c.derived));
    v.insert("kind".into(), json!(kind));
    v.insert("target".into(), json!(target));
    v.insert("expect".into(), json!(expect));
    v.insert("wait".into(), json!(c.wait));
    if let Probe::Http { request, .. } = &c.probe
        && request.follow
    {
        v.insert("follow".into(), json!(true));
    }
    Value::Object(v)
}

/// A probe as (kind, target, expect), its hosts written as addresses in the spec's blocks.
fn probe_parts(spec: &Spec, probe: &Probe) -> (&'static str, String, String) {
    match probe {
        Probe::Http { url, expect, .. } => (
            "http",
            url.render(&host_address(spec, &url.host)),
            match expect {
                HttpExpect::Status(s) => s.to_string(),
                HttpExpect::Any => "any".into(),
                HttpExpect::Blocked => "blocked".into(),
            },
        ),
        Probe::Tcp { host, port, expect, .. } => (
            "tcp",
            format!("{}:{port}", host_address(spec, host)),
            match expect {
                TcpExpect::Open => "open".into(),
                TcpExpect::Blocked => "blocked".into(),
            },
        ),
        Probe::Exec { command, expect } => ("exec", command.clone(), expect.clone().unwrap_or_default()),
        Probe::Script { path } => ("script", path.clone(), String::new()),
        Probe::Playbook { path } => ("playbook", path.clone(), String::new()),
    }
}

/// A probe's host as written in the snapshot: a machine's address on the network it is probed
/// on, or the host as the check wrote it.
fn host_address(spec: &Spec, h: &Host) -> String {
    match h {
        Host::Literal(l) => l.clone(),
        Host::Machine { name, network } => {
            let octet = spec.machines.get(name).and_then(|m| m.networks.get(network));
            octet.map_or_else(|| name.clone(), |o| address(spec, network, *o).to_string())
        }
    }
}
