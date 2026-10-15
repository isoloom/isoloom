//! The placeholders of the message and of an app's templates, read with the spec's one grammar
//! ([`placeholder`](super::super::placeholder)) and resolved against the spec at validation: a
//! typo or an index (`ports.0`) fails here, not at run time. The message may use
//! `machines.<m>.ports.<name>.port`, `.published` and `.url` (an `http` or `https` port's),
//! `machines.<m>.addresses.<network>` and `cloud.outputs.<name>`; a template the machine's own
//! `machine.name` and `machine.addresses.<network>`, and no run-time value.

use super::super::Spec;
use super::super::placeholder::{self, Place, PortField, Reference};
use super::super::problem::{Code, FieldPath, Problem, Problems};
use crate::Error;

/// Every placeholder of the message.
pub(super) fn check(spec: &Spec, out: &mut Problems) {
    let Some(message) = &spec.message else { return };
    let at = FieldPath::new("message");
    for found in placeholder_problems(spec, None, message, Place::Message) {
        let code = if found.unclosed { Code::InvalidValue } else { Code::UnknownReference };
        let p = Problem::new(code, at.clone(), found.reason);
        out.push(if found.unclosed { p } else { p.fix(example(spec)) });
    }
}

/// A placeholder that names nothing the spec has, or can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// Why, naming the placeholder.
    pub reason: String,
    /// Whether it is a `{{` without its `}}` (else a path naming nothing there).
    pub unclosed: bool,
}

/// The placeholders of `text`, written at `place`, that name nothing the spec has, in order:
/// `machine` is the machine a template is for. Empty when each resolves.
#[must_use]
pub fn placeholder_problems(spec: &Spec, machine: Option<&str>, text: &str, place: Place) -> Vec<Unresolved> {
    placeholder::placeholders(text, place)
        .into_iter()
        .filter_map(|found| match found {
            Ok(r) => resolve(spec, machine, &r).err().map(|why| Unresolved {
                reason: format!("`{{{{ {r} }}}}`: {why}"),
                unclosed: false,
            }),
            Err(e) => Some(Unresolved {
                unclosed: matches!(e, Error::Unclosed),
                reason: e.to_string(),
            }),
        })
        .collect()
}

/// Whether `r` names something of the spec (`template_of`: the machine a template is for),
/// else why not. An entry's own paths are the catalogue's to resolve.
fn resolve(spec: &Spec, template_of: Option<&str>, r: &Reference) -> Result<(), String> {
    let on = |m: &str, n: &str| -> Result<(), String> {
        let found = spec.machines.get(m).ok_or_else(|| format!("no machine named `{m}`"))?;
        if found.networks.contains_key(n) {
            Ok(())
        } else {
            Err(format!("`{m}` isn't on a network named `{n}`"))
        }
    };
    match r {
        Reference::OwnAddress { network } => on(template_of.unwrap_or_default(), network),
        Reference::Address { machine, network } => on(machine, network),
        Reference::Port { machine, port, field } => {
            let m = spec.machines.get(machine).ok_or_else(|| format!("no machine named `{machine}`"))?;
            let p = m
                .ports
                .get(port)
                .ok_or_else(|| format!("`{machine}` has no port named `{port}` (ports are named, never numbered)"))?;
            if *field == PortField::Url && !p.protocol.is_http() {
                return Err(format!("`{port}` isn't an `http` or `https` port, so it has no URL"));
            }
            Ok(())
        }
        Reference::CloudOutput(name) => match &spec.cloud {
            Some(c) if c.outputs.contains_key(name) => Ok(()),
            Some(_) => Err(format!("`cloud.outputs` has no `{name}`")),
            None => Err("the spec has no `cloud:`".into()),
        },
        _ => Ok(()),
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
