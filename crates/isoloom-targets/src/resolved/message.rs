//! The spec's `message`, its `{{ path }}` placeholders filled from the resolved snapshot, as it
//! reads on the target the environment runs on, and as the run manifest holds it.

use std::num::NonZeroU16;

use isoloom_manifest::{Name, Part, Placeholder, Template};
use serde_json::Value;

use super::resolve_with;
use crate::model::{Protocol as SpecProtocol, Spec, Target};

/// The spec's `message` with its `{{ path }}` placeholders filled from the snapshot; an error
/// names a placeholder that points at nothing.
pub fn render_message(spec: &Spec, instance: Option<u8>) -> Result<Option<String>, String> {
    let Some(m) = &spec.message else { return Ok(None) };
    let snapshot = resolve_with(spec, instance);
    fill(m, &snapshot).map(Some)
}

/// The message as it reads where the environment runs on `target`: on the targets that run
/// the Compose file, machines are at their Docker addresses, so `machines.<m>.addresses` is
/// filled with those (see [`on_target`]).
pub fn render_message_on(spec: &Spec, instance: Option<u8>, target: Target) -> Result<Option<String>, String> {
    render_message_at(spec, instance, target, &[])
}

/// [`render_message_on`] with the host ports the environment really got, as (machine, port,
/// protocol, host port): local Docker publishes on free ports unless `ISOLOOM_PUBLISH_FIXED` is set, so
/// `{{ machines.web.ports.0.publish }}` must say where it answers (see [`with_published`]).
pub fn render_message_at(spec: &Spec, instance: Option<u8>, target: Target, published: &[(String, u16, SpecProtocol, u16)]) -> Result<Option<String>, String> {
    let Some(m) = &spec.message else { return Ok(None) };
    fill(m, &with_published(on_target(resolve_with(spec, instance), target), published)).map(Some)
}

/// The snapshot with the host ports the environment really got, as (machine, port, protocol,
/// host port): each port's `publish` and the `published` list's `host_port`.
pub fn with_published(mut snapshot: Value, published: &[(String, u16, SpecProtocol, u16)]) -> Value {
    // A port without `protocol` in the snapshot is TCP.
    let same = |v: &Value, port: u16, proto: SpecProtocol| v["port"] == port && v["protocol"].as_str().unwrap_or("tcp") == proto.id();
    for (machine, port, proto, host) in published {
        let ports = snapshot.pointer_mut(&format!("/machines/{machine}/ports")).and_then(Value::as_array_mut);
        for s in ports.into_iter().flatten().filter(|s| same(s, *port, *proto)) {
            s["publish"] = (*host).into();
        }
        let listed = snapshot.get_mut("published").and_then(Value::as_array_mut);
        for p in listed
            .into_iter()
            .flatten()
            .filter(|p| p["machine"] == machine.as_str() && same(p, *port, *proto))
        {
            p["host_port"] = (*host).into();
        }
    }
    snapshot
}

/// The snapshot as seen on `target`: on the Compose targets (Docker, a hosting service, Docker
/// on a VM or a cloud VM) each machine's `addresses` are its `docker_addresses`, the ones it
/// really has there (they differ when the spec's networks are outside 10.0.0.0/8, or for an
/// instance); elsewhere the snapshot is unchanged.
pub fn on_target(mut snapshot: Value, target: Target) -> Value {
    if !matches!(target, Target::Docker | Target::Hosted | Target::CloudDocker | Target::DockerVm) {
        return snapshot;
    }
    if let Some(machines) = snapshot.get_mut("machines").and_then(Value::as_object_mut) {
        for m in machines.values_mut() {
            if let Some(docker) = m.get("docker_addresses").cloned() {
                m["addresses"] = docker;
            }
        }
    }
    snapshot
}

/// A message's literal text and `{{ path }}` placeholders, in order.
enum Piece<'a> {
    Text(&'a str),
    Path(&'a str),
}

/// The pieces of `text`; an error names a `{{` without its `}}`.
fn pieces(text: &str) -> Result<Vec<Piece<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push(Piece::Text(&rest[..start]));
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err("message: a `{{` without its `}}`".into());
        };
        out.push(Piece::Path(after[..end].trim()));
        rest = &after[end + 2..];
    }
    out.push(Piece::Text(rest));
    Ok(out)
}

/// The snapshot's value at `path` as a message shows it: scalars plainly, lists and maps as
/// compact JSON; an error names a path that points at nothing.
fn shown(snapshot: &Value, path: &str) -> Result<String, String> {
    let value = lookup(snapshot, path).ok_or_else(|| {
        // A machine's `services` is `ports` since 1.0, in the snapshot too.
        let renamed = if path.contains(".services.") {
            "; a machine's `services` is now `ports`"
        } else {
            ""
        };
        format!("message: `{{{{ {path} }}}}` points at nothing in the snapshot (see `isoloom inspect`){renamed}")
    })?;
    Ok(match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Fills `{{ dotted.path }}` placeholders from a snapshot. Scalars print plainly; lists and
/// maps as compact JSON.
pub fn fill(text: &str, snapshot: &Value) -> Result<String, String> {
    pieces(text)?
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Ok(t.to_string()),
            Piece::Path(path) => shown(snapshot, path),
        })
        .collect()
}

/// The message as the run manifest holds it on `target`: what is known when the files are
/// generated filled in, and each published port's host port left to the runner (`{{
/// machines.web.ports.0.publish }}` becomes `${published.web.80}`). `None` without a message, or
/// when a placeholder points at nothing (the snapshot's message is null then too).
pub fn manifest_message(spec: &Spec, instance: Option<u8>, target: Target) -> Option<Template> {
    let text = spec.message.as_ref()?;
    let snapshot = on_target(resolve_with(spec, instance), target);
    let parts = pieces(text).ok()?.into_iter().map(|p| match p {
        Piece::Text(t) => Ok(Part::Text(t.to_string())),
        Piece::Path(path) => match published(&snapshot, path) {
            Some(port) => Ok(Part::Placeholder(port)),
            None => shown(&snapshot, path).map(Part::Text),
        },
    });
    parts.collect::<Result<Vec<Part>, String>>().ok().map(Template::from_parts)
}

/// The published port a path names (`machines.<m>.ports.<i>.publish`,
/// `published.<i>.host_port`), as the manifest's placeholder for its host port.
fn published(snapshot: &Value, path: &str) -> Option<Placeholder> {
    let keys: Vec<&str> = path.split('.').filter(|k| !k.is_empty()).collect();
    let (machine, port) = match keys.as_slice() {
        ["machines", m, "ports", i, "publish"] => {
            let port = snapshot.pointer(&format!("/machines/{m}/ports/{i}"))?;
            port.get("publish").filter(|p| !p.is_null())?;
            ((*m).to_string(), port)
        }
        ["published", i, "host_port"] => {
            let port = snapshot.pointer(&format!("/published/{i}"))?;
            (port["machine"].as_str()?.to_string(), port)
        }
        _ => return None,
    };
    Some(Placeholder::Published {
        machine: Name::parse(&machine).ok()?,
        port: u16::try_from(port["port"].as_u64()?).ok().and_then(NonZeroU16::new)?,
        protocol: if port["protocol"].as_str() == Some("udp") {
            isoloom_manifest::Protocol::Udp
        } else {
            isoloom_manifest::Protocol::Tcp
        },
    })
}

/// A dotted path into the snapshot (`machines.web.addresses`, `checks.positions.0.runner`).
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|s| !s.is_empty()).try_fold(value, |v, key| match v {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}
