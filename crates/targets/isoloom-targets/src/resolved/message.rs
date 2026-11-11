//! The spec's `message`, its `{{ path }}` placeholders (the spec's one grammar,
//! [`isoloom_spec::syntax::placeholder`]) filled from the resolved snapshot, as it reads on the
//! target the environment runs on, and as the run manifest holds it.

use isoloom_manifest::{Name, Part, Placeholder, PortName, SnakeName, Template};
use isoloom_plan::Lab;
use isoloom_spec::Error as SpecError;
use isoloom_spec::model::Target;
use isoloom_spec::syntax::placeholder::{self, Part as SpecPart, Place, PortField, Reference};
use serde_json::Value;

use super::resolve_with;
use crate::{Error, Result};

/// The environment's `message` with its `{{ path }}` placeholders filled from the snapshot; an error
/// names a placeholder that points at nothing.
///
/// # Errors
///
/// [`Error::Instance`] when the environment can't be made that instance, [`Error::Placeholder`] for a
/// placeholder the message can't have or that points at nothing in the snapshot, and
/// [`Error::Unclosed`] for a `{{` without its `}}`.
pub fn render_message(lab: &Lab, instance: Option<u8>) -> Result<Option<String>> {
    let Some(m) = &lab.source().message else { return Ok(None) };
    fill(m, &resolve_with(lab, instance)?).map(Some)
}

/// The message as it reads where the environment runs on `target`: on the targets that run
/// the Compose file, machines are at their Docker addresses, so `machines.<m>.addresses` is
/// filled with those (see `on_target`).
///
/// # Errors
///
/// As [`render_message`].
pub fn render_message_on(lab: &Lab, instance: Option<u8>, target: Target) -> Result<Option<String>> {
    let Some(m) = &lab.source().message else { return Ok(None) };
    fill(m, &on_target(resolve_with(lab, instance)?, target)).map(Some)
}

/// The snapshot as seen on `target`: on the Compose targets (Docker, a hosting service, Docker
/// on a VM or a cloud VM) each machine's `addresses` are its `docker_addresses`, the ones it
/// really has there (they differ when the spec's networks are outside 10.0.0.0/8, or for an
/// instance); elsewhere the snapshot is unchanged.
#[must_use]
pub(crate) fn on_target(mut snapshot: Value, target: Target) -> Value {
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

/// The message's text and placeholders, read by the spec's one placeholder grammar; the first
/// placeholder it doesn't have, or a `{{` without its `}}`, is the error.
fn parts(text: &str) -> Result<Vec<SpecPart>> {
    placeholder::read(text, Place::Message).map_err(|wrong| match wrong.into_iter().next() {
        Some(SpecError::Placeholder { path, why }) => Error::Placeholder { path, why: why.to_owned() },
        _ => Error::Unclosed,
    })
}

/// What placeholder `r` shows, from the snapshot: its value, scalars plainly, lists and maps as
/// compact JSON; a cloud output, known only once the module is applied, as written.
/// [`Error::Placeholder`] for one that points at nothing.
fn shown(snapshot: &Value, r: &Reference) -> Result<String> {
    if let Reference::CloudOutput(_) = r {
        return Ok(format!("{{{{ {r} }}}}"));
    }
    let path = r.to_string();
    let value = lookup(snapshot, &path).ok_or_else(|| Error::Placeholder {
        path,
        why: "points at nothing in the snapshot (see `isoloom inspect`)".to_owned(),
    })?;
    Ok(match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Fills a message's `{{ path }}` placeholders from a snapshot. Scalars print plainly; lists and
/// maps as compact JSON; a cloud output stays as written.
///
/// # Errors
///
/// As [`render_message`].
pub fn fill(text: &str, snapshot: &Value) -> Result<String> {
    placeholder::join(parts(text)?, |r| shown(snapshot, r))
}

/// The message as the run manifest holds it on `target`: what is known when the files are
/// generated filled in, and what only the runner knows left to it: each published port's host
/// port (`{{ machines.web.ports.site.published }}` becomes `${published.web.site}`, and its `.url`
/// the page at it) and each cloud output (`{{ cloud.outputs.site }}` becomes `${outputs.site}`).
/// `None` without a message, or when a placeholder points at nothing in the snapshot (the
/// snapshot's message is null then too).
///
/// # Errors
///
/// As [`render_message`], but a placeholder that points at nothing: the environment can't be made
/// instance `instance`, or the message can't be read.
pub(crate) fn manifest_message(lab: &Lab, instance: Option<u8>, target: Target) -> Result<Option<Template>> {
    let Some(text) = lab.source().message.as_ref() else { return Ok(None) };
    let snapshot = on_target(resolve_with(lab, instance)?, target);
    let mut parts = Vec::new();
    for part in self::parts(text)? {
        match part {
            SpecPart::Text(t) => parts.push(Part::Text(t)),
            SpecPart::Placeholder(r) => match at_run_time(&snapshot, &r) {
                Some(found) => parts.extend(found),
                None => match shown(&snapshot, &r) {
                    Ok(value) => parts.push(Part::Text(value)),
                    Err(Error::Placeholder { .. }) => return Ok(None),
                    Err(other) => return Err(other),
                },
            },
        }
    }
    Ok(Some(Template::from_parts(parts)))
}

/// What a placeholder only the runner knows reads as in the manifest: a cloud output; a published
/// port's host port, or its page (`.url`) with the host port left to the runner.
fn at_run_time(snapshot: &Value, r: &Reference) -> Option<Vec<Part>> {
    let (machine, port, field) = match r {
        Reference::CloudOutput(name) => return Some(vec![Part::Placeholder(Placeholder::Output(SnakeName::parse(name).ok()?))]),
        Reference::Port { machine, port, field } if *field != PortField::Port => (machine, port, field),
        _ => return None,
    };
    let found = snapshot.pointer(&format!("/machines/{machine}/ports/{port}"))?;
    found.get("published").filter(|p| !p.is_null())?;
    let host = Part::Placeholder(Placeholder::Published {
        machine: Name::parse(machine).ok()?,
        port: PortName::parse(port.as_str()).ok()?,
    });
    if *field == PortField::Published {
        return Some(vec![host]);
    }
    let scheme = if found.get("protocol").and_then(Value::as_str) == Some("https") {
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
        let snapshot = json!({ "machines": { "web": { "ports": { "site": { "port": 80 } }, "addresses": { "lab": ["a"] } } } });
        assert_eq!(
            fill("port {{ machines.web.ports.site.port }}, at {{machines.web.addresses.lab}}", &snapshot)?,
            "port 80, at [\"a\"]"
        );
        assert!(matches!(fill("{{ machines.db.addresses.lab }}", &snapshot), Err(Error::Placeholder { .. })));
        assert!(matches!(fill("{{ machines.db }}", &snapshot), Err(Error::Placeholder { .. })));
        Ok(())
    }

    #[test]
    fn the_manifests_message_fails_on_a_real_error_and_is_none_for_a_placeholder_that_points_at_nothing() {
        let lab = |message: &str| {
            Lab::parse(&format!(
                "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, ports: {{ site: 80 }}, app: {{ image: nginx }} }}\nmessage: \"{message}\"\n"
            ))
            .unwrap()
        };
        let at = lab("at {{ machines.web.addresses.lan }}");
        let filled = manifest_message(&at, None, Target::Docker).unwrap().unwrap();
        assert_eq!(filled.parts(), [Part::Text("at 10.9.0.10".into())]);
        assert!(matches!(manifest_message(&at, Some(0), Target::Docker), Err(Error::Instance { number: 0, .. })));
        assert!(matches!(
            manifest_message(&lab("at {{ machines.web"), None, Target::Docker),
            Err(Error::Unclosed)
        ));
        assert!(
            manifest_message(&lab("at {{ machines.web.addresses.wan }}"), None, Target::Docker)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_path_reads_through_lists_by_index() {
        assert_eq!(lookup(&json!({ "a": [{ "b": 1 }] }), "a.0.b"), Some(&json!(1)));
    }

    proptest! {
        #[test]
        fn text_without_a_placeholder_reads_as_itself(text in "[^{]*") {
            prop_assert_eq!(fill(&text, &Value::Null).ok(), Some(text.clone()));
        }

        #[test]
        fn a_placeholder_between_any_texts_is_replaced_by_its_value(before in "[^{]*", after in "[^{]*", value in "[a-z0-9 ]{0,12}") {
            let snapshot = json!({ "machines": { "a": { "addresses": { "b": value.clone() } } } });
            let text = format!("{before}{{{{ machines.a.addresses.b }}}}{after}");
            prop_assert_eq!(fill(&text, &snapshot).ok(), Some(format!("{before}{value}{after}")));
        }

        #[test]
        fn an_open_placeholder_is_an_error(before in "[^{]*", after in "[^}]*") {
            let open = fill(&format!("{before}{{{{{after}"), &Value::Null);
            prop_assert!(matches!(open, Err(Error::Unclosed)));
        }
    }
}
