//! The spec's `message`, its `{{ path }}` placeholders filled from the resolved snapshot, as it
//! reads on the target the environment runs on, and as the run manifest holds it.

use isoloom_manifest::{Name, Part, Placeholder, PortName, Template};
use isoloom_plan::Lab;
use isoloom_spec::model::Target;
use serde_json::Value;

use super::resolve_with;
use crate::{Error, Result};

/// The lab's `message` with its `{{ path }}` placeholders filled from the snapshot; an error
/// names a placeholder that points at nothing.
///
/// # Errors
///
/// [`Error::Placeholder`] for a placeholder that points at nothing in the snapshot, and
/// [`Error::Unclosed`] for a `{{` without its `}}`.
pub fn render_message(lab: &Lab, instance: Option<u8>) -> Result<Option<String>> {
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
pub fn render_message_on(lab: &Lab, instance: Option<u8>, target: Target) -> Result<Option<String>> {
    render_message_at(lab, instance, target, &[])
}

/// A machine's port and the host port a runner published it on: local Docker publishes on free
/// ports unless `ISOLOOM_PUBLISH_FIXED` is set, so the message must say where it answers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostPort {
    /// The machine.
    pub machine: String,
    /// The port's name (`site`).
    pub port: String,
    /// The host port it got.
    pub host: u16,
}

/// [`render_message_on`] with the host ports the environment really got, so
/// `{{ machines.web.ports.site.published }}` says where it answers (see [`with_published`]).
///
/// # Errors
///
/// As [`render_message`].
pub fn render_message_at(lab: &Lab, instance: Option<u8>, target: Target, published: &[HostPort]) -> Result<Option<String>> {
    let Some(m) = &lab.source().message else { return Ok(None) };
    fill(m, &with_published(on_target(resolve_with(lab, instance), target), published)).map(Some)
}

/// The snapshot with the host ports the environment really got: each port's `published` and
/// `url`, and the `published` list's `host_port`.
#[must_use]
pub fn with_published(mut snapshot: Value, published: &[HostPort]) -> Value {
    for HostPort { machine, port, host } in published {
        let entry = snapshot
            .pointer_mut(&format!("/machines/{machine}/ports/{port}"))
            .and_then(Value::as_object_mut);
        if let Some(p) = entry {
            p.insert("published".into(), (*host).into());
            if let Some(url) = p.get("url").and_then(Value::as_str).map(|u| with_port(u, *host)) {
                p.insert("url".into(), url.into());
            }
        }
        let listed = snapshot.get_mut("published").and_then(Value::as_array_mut);
        for p in listed.into_iter().flatten().filter_map(Value::as_object_mut) {
            let named = |key: &str, want: &str| p.get(key).and_then(Value::as_str) == Some(want);
            if named("machine", machine) && named("name", port) {
                p.insert("host_port".into(), (*host).into());
            }
        }
    }
    snapshot
}

/// URL `url` (`http://127.0.0.1:8080/`) on host port `host` instead.
fn with_port(url: &str, host: u16) -> String {
    match url.rsplit_once(':') {
        Some((head, _)) => format!("{head}:{host}/"),
        None => url.to_owned(),
    }
}

/// The snapshot as seen on `target`: on the Compose targets (Docker, a hosting service, Docker
/// on a VM or a cloud VM) each machine's `addresses` are its `docker_addresses`, the ones it
/// really has there (they differ when the spec's networks are outside 10.0.0.0/8, or for an
/// instance); elsewhere the snapshot is unchanged.
#[must_use]
pub fn on_target(mut snapshot: Value, target: Target) -> Value {
    if !matches!(target, Target::Docker | Target::Hosted | Target::CloudDocker | Target::DockerVm) {
        return snapshot;
    }
    if let Some(machines) = snapshot.get_mut("machines").and_then(Value::as_object_mut) {
        for m in machines.values_mut() {
            if let Some(m) = m.as_object_mut()
                && let Some(docker) = m.get("docker_addresses").cloned()
            {
                m.insert("addresses".into(), docker);
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

/// The pieces of `text`, or [`Error::Unclosed`] for a `{{` without its `}}`.
fn pieces(text: &str) -> Result<Vec<Piece<'_>>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{{") {
        out.push(Piece::Text(before));
        let (path, tail) = after.split_once("}}").ok_or(Error::Unclosed)?;
        out.push(Piece::Path(path.trim()));
        rest = tail;
    }
    out.push(Piece::Text(rest));
    Ok(out)
}

/// The snapshot's value at `path` as a message shows it: scalars plainly, lists and maps as
/// compact JSON; [`Error::Placeholder`] for a path that points at nothing.
fn shown(snapshot: &Value, path: &str) -> Result<String> {
    let value = lookup(snapshot, path).ok_or_else(|| Error::Placeholder {
        path: path.to_owned(),
        // A machine's `services` is `ports` since 1.0, in the snapshot too.
        hint: if path.contains(".services.") {
            "; a machine's `services` is now `ports`"
        } else {
            ""
        },
    })?;
    Ok(match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Fills `{{ dotted.path }}` placeholders from a snapshot. Scalars print plainly; lists and
/// maps as compact JSON.
///
/// # Errors
///
/// As [`render_message`].
pub fn fill(text: &str, snapshot: &Value) -> Result<String> {
    pieces(text)?
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Ok(t.to_owned()),
            Piece::Path(path) => shown(snapshot, path),
        })
        .collect()
}

/// The message as the run manifest holds it on `target`: what is known when the files are
/// generated filled in, and each published port's host port left to the runner (`{{
/// machines.web.ports.site.published }}` becomes `${published.web.site}`, and its `.url` the
/// page at it). `None` without a message, or when a placeholder points at nothing (the
/// snapshot's message is null then too).
pub(crate) fn manifest_message(lab: &Lab, instance: Option<u8>, target: Target) -> Option<Template> {
    let text = lab.source().message.as_ref()?;
    let snapshot = on_target(resolve_with(lab, instance), target);
    let mut parts = Vec::new();
    for piece in pieces(text).ok()? {
        match piece {
            Piece::Text(t) => parts.push(Part::Text(t.to_owned())),
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
        port: PortName::parse(name).ok()?,
    });
    if *field == "published" {
        return Some(vec![host]);
    }
    let scheme = if port.get("protocol").and_then(Value::as_str) == Some("https") {
        "https"
    } else {
        "http"
    };
    Some(vec![Part::Text(format!("{scheme}://127.0.0.1:")), host, Part::Text("/".into())])
}

/// A dotted path into the snapshot (`machines.web.addresses`, `checks.positions.0.runner`).
#[must_use]
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|s| !s.is_empty()).try_fold(value, |v, key| match v {
        Value::Object(m) => m.get(key),
        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    #[test]
    fn a_placeholder_is_filled_from_its_dotted_path() -> Result<()> {
        let snapshot = json!({ "machines": { "web": { "ports": { "site": { "port": 80 } }, "tags": ["a"] } } });
        assert_eq!(
            fill("port {{ machines.web.ports.site.port }}, tags {{machines.web.tags}}", &snapshot)?,
            "port 80, tags [\"a\"]"
        );
        assert!(matches!(fill("{{ machines.db }}", &snapshot), Err(Error::Placeholder { .. })));
        Ok(())
    }

    #[test]
    fn a_published_port_moves_its_url_to_the_host_port_it_got() {
        assert_eq!(with_port("http://127.0.0.1:8080/", 32772), "http://127.0.0.1:32772/");
        assert_eq!(lookup(&json!({ "a": [{ "b": 1 }] }), "a.0.b"), Some(&json!(1)));
    }

    proptest! {
        #[test]
        fn text_without_a_placeholder_reads_as_itself(text in "[^{]*") {
            prop_assert_eq!(fill(&text, &Value::Null).ok(), Some(text.clone()));
        }

        #[test]
        fn a_placeholder_between_any_texts_is_replaced_by_its_value(before in "[^{]*", after in "[^{]*", value in "[a-z0-9 ]{0,12}") {
            let snapshot = json!({ "a": { "b": value.clone() } });
            prop_assert_eq!(fill(&format!("{before}{{{{ a.b }}}}{after}"), &snapshot).ok(), Some(format!("{before}{value}{after}")));
        }

        #[test]
        fn an_open_placeholder_is_an_error(before in "[^{]*", after in "[^}]*") {
            let open = fill(&format!("{before}{{{{{after}"), &Value::Null);
            prop_assert!(matches!(open, Err(Error::Unclosed)));
        }
    }
}
