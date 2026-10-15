//! The spec's `message`, its `{{ path }}` placeholders filled from the resolved snapshot, as it
//! reads on the target the environment runs on, and as the run manifest holds it.

use isoloom_manifest::{Name, Part, Placeholder, Template};
use serde_json::Value;

use super::{Lab, resolve_with};
use crate::model::Target;

/// The lab's `message` with its `{{ path }}` placeholders filled from the snapshot; an error
/// names a placeholder that points at nothing.
///
/// # Errors
///
/// A placeholder that points at nothing in the snapshot, or a `{{` without its `}}`.
pub fn render_message(lab: &Lab, instance: Option<u8>) -> Result<Option<String>, String> {
    let Some(m) = &lab.source().message else { return Ok(None) };
    fill(m, &resolve_with(lab, instance)).map(Some)
}

/// The message as it reads where the environment runs on `target`: on the targets that run
/// the Compose file, machines are at their Docker addresses, so `machines.<m>.addresses` is
/// filled with those (see [`on_target`]).
///
/// # Errors
///
/// As [`render_message`].
pub fn render_message_on(lab: &Lab, instance: Option<u8>, target: Target) -> Result<Option<String>, String> {
    render_message_at(lab, instance, target, &[])
}

/// [`render_message_on`] with the host ports the environment really got, as (machine, port
/// name, host port): local Docker publishes on free ports unless `ISOLOOM_PUBLISH_FIXED` is set,
/// so `{{ machines.web.ports.site.published }}` must say where it answers (see
/// [`with_published`]).
///
/// # Errors
///
/// As [`render_message`].
pub fn render_message_at(lab: &Lab, instance: Option<u8>, target: Target, published: &[(String, String, u16)]) -> Result<Option<String>, String> {
    let Some(m) = &lab.source().message else { return Ok(None) };
    fill(m, &with_published(on_target(resolve_with(lab, instance), target), published)).map(Some)
}

/// The snapshot with the host ports the environment really got, as (machine, port name, host
/// port): each port's `published` and `url`, and the `published` list's `host_port`.
#[must_use]
pub fn with_published(mut snapshot: Value, published: &[(String, String, u16)]) -> Value {
    for (machine, name, host) in published {
        if let Some(p) = snapshot.pointer_mut(&format!("/machines/{machine}/ports/{name}")) {
            p["published"] = (*host).into();
            if let Some(url) = p.get("url").and_then(Value::as_str).map(|u| with_port(u, *host)) {
                p["url"] = url.into();
            }
        }
        let listed = snapshot.get_mut("published").and_then(Value::as_array_mut);
        for p in listed
            .into_iter()
            .flatten()
            .filter(|p| p["machine"] == machine.as_str() && p["name"] == name.as_str())
        {
            p["host_port"] = (*host).into();
        }
    }
    snapshot
}

/// URL `url` (`http://127.0.0.1:8080/`) on host port `host` instead.
fn with_port(url: &str, host: u16) -> String {
    match url.rsplit_once(':') {
        Some((head, _)) => format!("{head}:{host}/"),
        None => url.to_string(),
    }
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
/// machines.web.ports.site.published }}` becomes `${published.web.site}`, and its `.url` the
/// page at it). `None` without a message, or when a placeholder points at nothing (the
/// snapshot's message is null then too).
#[must_use]
pub fn manifest_message(lab: &Lab, instance: Option<u8>, target: Target) -> Option<Template> {
    let text = lab.source().message.as_ref()?;
    let snapshot = on_target(resolve_with(lab, instance), target);
    let mut parts = Vec::new();
    for piece in pieces(text).ok()? {
        match piece {
            Piece::Text(t) => parts.push(Part::Text(t.to_string())),
            Piece::Path(path) => match published(&snapshot, path) {
                Some(found) => parts.extend(found),
                None => parts.push(Part::Text(shown(&snapshot, path).ok()?)),
            },
        }
    }
    Some(Template::from_parts(parts))
}

/// What a path to a published port reads as in the manifest: its host port
/// (`machines.<m>.ports.<name>.published`) or its page (`.url`) with the host port left to the
/// runner.
fn published(snapshot: &Value, path: &str) -> Option<Vec<Part>> {
    let keys: Vec<&str> = path.split('.').filter(|k| !k.is_empty()).collect();
    let ["machines", m, "ports", name, field @ ("published" | "url")] = keys.as_slice() else {
        return None;
    };
    let port = snapshot.pointer(&format!("/machines/{m}/ports/{name}"))?;
    port.get("published").filter(|p| !p.is_null())?;
    let host = Part::Placeholder(Placeholder::Published {
        machine: Name::parse(m).ok()?,
        port: Name::parse(name).ok()?,
    });
    if *field == "published" {
        return Some(vec![host]);
    }
    let scheme = if port["protocol"] == "https" { "https" } else { "http" };
    Some(vec![Part::Text(format!("{scheme}://127.0.0.1:")), host, Part::Text("/".into())])
}

/// A dotted path into the snapshot (`machines.web.addresses`, `checks.positions.0.runner`).
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|s| !s.is_empty()).try_fold(value, |v, key| match v {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}
