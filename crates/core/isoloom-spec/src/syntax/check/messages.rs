//! The placeholders of the message and of an app's templates, read with the spec's one grammar
//! ([`placeholder`]) and resolved against the spec at validation: a
//! typo or an index (`ports.0`) fails here, not at run time. The message may use
//! `machines.<m>.ports.<name>.port`, and a published port's `.published` and `.url` (an `http`
//! or `https` one's),
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
        let p = Problem::new(found.code(), at.clone(), found.reason().to_owned());
        out.push(match found {
            Unresolved::Unclosed(_) => p,
            Unresolved::Unknown(_) => p.fix(example(spec)),
        });
    }
}

/// A placeholder that can't be read, or names nothing the spec has; each says why, naming the
/// placeholder (a flag told them apart before, and every reader branched on it: Clean Code F3,
/// 2026-11-13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// A `{{` without its `}}`.
    Unclosed(String),
    /// A path naming nothing the spec has.
    Unknown(String),
}

impl Unresolved {
    /// Why, naming the placeholder.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Unresolved::Unclosed(reason) | Unresolved::Unknown(reason) => reason,
        }
    }

    /// The problem it is: an unclosed placeholder is an invalid value, an unknown path a
    /// reference to nothing.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Unresolved::Unclosed(_) => Code::InvalidValue,
            Unresolved::Unknown(_) => Code::UnknownReference,
        }
    }
}

/// The placeholders of `text`, written at `place`, that name nothing the spec has, in order:
/// `machine` is the machine a template is for. Empty when each resolves.
#[must_use]
pub fn placeholder_problems(spec: &Spec, machine: Option<&str>, text: &str, place: Place) -> Vec<Unresolved> {
    placeholder::placeholders(text, place)
        .into_iter()
        .filter_map(|found| match found {
            Ok(r) => resolve(spec, machine, &r)
                .err()
                .map(|why| Unresolved::Unknown(format!("`{{{{ {r} }}}}`: {why}"))),
            Err(Error::Unclosed) => Some(Unresolved::Unclosed(Error::Unclosed.to_string())),
            Err(e) => Some(Unresolved::Unknown(e.to_string())),
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
            if *field != PortField::Port && p.publish.is_none() {
                return Err(format!(
                    "`{port}` isn't published (`publish:`), so it has no published port or URL; `.port` is its number in the machine"
                ));
            }
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

/// A placeholder the spec could write, as the fix: a published port's published port, else a
/// port's number, else a machine's address, each of the spec's own; one that resolves.
fn example(spec: &Spec) -> String {
    let machines = || spec.machines.iter();
    let published = machines().find_map(|(m, machine)| {
        let (p, _) = machine.ports.iter().find(|(_, p)| p.publish.is_some())?;
        Some(format!("machines.{m}.ports.{p}.published"))
    });
    let port = || machines().find_map(|(m, machine)| Some(format!("machines.{m}.ports.{}.port", machine.ports.keys().next()?)));
    let address = || machines().find_map(|(m, machine)| Some(format!("machines.{m}.addresses.{}", machine.networks.keys().next()?)));
    published.or_else(port).or_else(address).map_or_else(
        || "`{{ machines.<machine>.ports.<name>.published }}` or `{{ machines.<machine>.addresses.<network> }}`".into(),
        |path| format!("`{{{{ {path} }}}}`"),
    )
}
