//! The message's placeholders, resolved against the spec at validation: a typo or an index
//! (`ports.0`) fails here, not at run time. The paths a message may use:
//! `machines.<m>.ports.<name>.port`, `.published` and `.url` (an `http` or `https` port's),
//! `machines.<m>.addresses.<network>` and `cloud.outputs.<name>`.

use super::super::Spec;
use super::super::problem::{Code, FieldPath, Problem, Problems};
use crate::placeholders::{self, Piece};

/// Every placeholder of the message.
pub(super) fn check(spec: &Spec, out: &mut Problems) {
    let Some(message) = &spec.message else { return };
    let at = FieldPath::new("message");
    for piece in placeholders::pieces(message) {
        match piece {
            Piece::Text(_) => {}
            Piece::Placeholder(inner) => {
                let path = inner.trim();
                if let Err(why) = resolve(spec, path) {
                    out.push(Problem::new(Code::UnknownReference, at.clone(), format!("`{{{{ {path} }}}}`: {why}")).fix(example(spec)));
                }
            }
            Piece::Unclosed(_) => out.push(Problem::new(Code::InvalidValue, at.clone(), "a `{{` without its `}}`")),
        }
    }
}

/// Whether `path` names something the message can show, else why not.
fn resolve(spec: &Spec, path: &str) -> Result<(), String> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["cloud", "outputs", name] => match &spec.cloud {
            Some(c) if c.outputs.contains_key(*name) => Ok(()),
            Some(_) => Err(format!("`cloud.outputs` has no `{name}`")),
            None => Err("the spec has no `cloud:`".into()),
        },
        ["machines", m, rest @ ..] => {
            let machine = spec.machines.get(*m).ok_or_else(|| format!("no machine named `{m}`"))?;
            match rest {
                ["addresses", n] if machine.networks.contains_key(*n) => Ok(()),
                ["addresses", n] => Err(format!("`{m}` isn't on a network named `{n}`")),
                ["ports", p, field] => {
                    let port = machine.ports.iter().find(|(k, _)| k.as_str() == *p).map(|(_, v)| v);
                    match (port, *field) {
                        (None, _) => Err(format!("`{m}` has no port named `{p}` (ports are named, never numbered)")),
                        (Some(_), "port" | "published") => Ok(()),
                        (Some(port), "url") if port.protocol.is_http() => Ok(()),
                        (Some(_), "url") => Err(format!("`{p}` isn't an `http` or `https` port, so it has no URL")),
                        (Some(_), other) => Err(format!("a port has `port`, `published` and `url`, not `{other}`")),
                    }
                }
                _ => Err("a machine's paths are `ports.<name>.port`, `.published`, `.url` and `addresses.<network>`".into()),
            }
        }
        _ => Err("not a path the message can show".into()),
    }
}

/// A placeholder the spec could write, as the fix.
fn example(spec: &Spec) -> String {
    let named = spec
        .machines
        .iter()
        .find_map(|(m, machine)| machine.ports.keys().next().map(|p| format!("{{{{ machines.{m}.ports.{p}.published }}}}")));
    named.unwrap_or_else(|| "`{{ machines.<machine>.ports.<name>.published }}` or `{{ machines.<machine>.addresses.<network> }}`".into())
}
