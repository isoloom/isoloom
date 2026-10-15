//! The message's placeholders, resolved against the spec at validation: a typo or an index
//! (`ports.0`) fails here, not at run time. The paths a message may use:
//! `machines.<m>.ports.<name>.port`, `.published` and `.url` (an `http` or `https` port's),
//! `machines.<m>.addresses.<network>` and `cloud.outputs.<name>`.

use super::super::Spec;
use super::super::machine::Machine;
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
                if let Some(why) = unresolved(spec, path) {
                    out.push(Problem::new(Code::UnknownReference, at.clone(), format!("`{{{{ {path} }}}}`: {why}")).fix(example(spec)));
                }
            }
            Piece::Unclosed(_) => out.push(Problem::new(Code::InvalidValue, at.clone(), "a `{{` without its `}}`")),
        }
    }
}

/// Why `path` names nothing the message can show; `None` when it names something.
fn unresolved(spec: &Spec, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["cloud", "outputs", name] => match &spec.cloud {
            Some(c) if c.outputs.contains_key(*name) => None,
            Some(_) => Some(format!("`cloud.outputs` has no `{name}`")),
            None => Some("the spec has no `cloud:`".into()),
        },
        ["machines", m, rest @ ..] => match spec.machines.get(*m) {
            None => Some(format!("no machine named `{m}`")),
            Some(machine) => unresolved_in(m, machine, rest),
        },
        _ => Some("not a path the message can show".into()),
    }
}

/// Why `rest`, a path inside machine `m`, names nothing; `None` when it names something.
fn unresolved_in(m: &str, machine: &Machine, rest: &[&str]) -> Option<String> {
    match rest {
        ["addresses", n] if machine.networks.contains_key(*n) => None,
        ["addresses", n] => Some(format!("`{m}` isn't on a network named `{n}`")),
        ["ports", p, field] => {
            let port = machine.ports.iter().find(|(k, _)| k.as_str() == *p).map(|(_, v)| v);
            match (port, *field) {
                (None, _) => Some(format!("`{m}` has no port named `{p}` (ports are named, never numbered)")),
                (Some(_), "port" | "published") => None,
                (Some(port), "url") if port.protocol.is_http() => None,
                (Some(_), "url") => Some(format!("`{p}` isn't an `http` or `https` port, so it has no URL")),
                (Some(_), other) => Some(format!("a port has `port`, `published` and `url`, not `{other}`")),
            }
        }
        _ => Some("a machine's paths are `ports.<name>.port`, `.published`, `.url` and `addresses.<network>`".into()),
    }
}

/// A placeholder the spec could write, as the fix.
fn example(spec: &Spec) -> String {
    let named = spec
        .machines
        .iter()
        .find_map(|(m, machine)| machine.ports.keys().next().map(|p| format!("`{{{{ machines.{m}.ports.{p}.published }}}}`")));
    named.unwrap_or_else(|| "`{{ machines.<machine>.ports.<name>.published }}` or `{{ machines.<machine>.addresses.<network> }}`".into())
}
