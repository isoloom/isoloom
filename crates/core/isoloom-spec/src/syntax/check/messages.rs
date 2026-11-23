//! The placeholders of the message and of an app's templates, read with the spec's one grammar
//! ([`placeholder`]) and resolved against the spec at validation: a typo or an index (`ports.0`)
//! fails here, not at run time, naming the nearest placeholder the spec has when there is one
//! (`machines.wbe.ports.site.url`: `machines.web.ports.site.url`). The message may use
//! `machines.<m>.ports.<name>.port`, a published port's `.published` and `.url` (an `http` or
//! `https` one's) and `machines.<m>.addresses.<network>`; a template those but the run-time
//! values, and the machine's own `machine.name` and `machine.addresses.<network>`.

use super::super::Spec;
use super::super::names::PortName;
use super::super::placeholder::{self, Place, PortField, Reference};
use super::super::problem::{Code, FieldPath, Problem, Problems, closest};
use crate::Error;

/// Every placeholder of the message.
pub(super) fn check(spec: &Spec, out: &mut Problems) {
    let Some(message) = &spec.message else { return };
    let at = FieldPath::new("message");
    for (found, nearest) in unresolved(spec, None, message, Place::Message) {
        let p = Problem::new(found.code(), at.clone(), found.reason().to_owned());
        out.push(match (found, nearest) {
            (Unresolved::Unclosed(_), _) => p,
            (Unresolved::Unknown(_), Some(near)) => p.fix(format!("`{{{{ {near} }}}}`")),
            (Unresolved::Unknown(_), None) => p.fix(example(spec)),
        });
    }
}

/// A placeholder that can't be read, or names nothing the spec has; each says why, naming the
/// placeholder, and which problem it is ([`Unresolved::code`]).
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
/// `machine` is the machine a template is for. Each names the nearest placeholder the spec has,
/// when one is near (`(did you mean `{{ machines.web.ports.site.port }}`?)`). Empty when each
/// resolves.
#[must_use]
pub fn placeholder_problems(spec: &Spec, machine: Option<&str>, text: &str, place: Place) -> Vec<Unresolved> {
    unresolved(spec, machine, text, place)
        .into_iter()
        .map(|(found, nearest)| match (found, nearest) {
            (Unresolved::Unknown(why), Some(near)) => Unresolved::Unknown(format!("{why} (did you mean `{{{{ {near} }}}}`?)")),
            (found, _) => found,
        })
        .collect()
}

/// The placeholders of `text`, written at `place`, that name nothing the spec has, in order, each
/// with the nearest path the spec has ([`nearest`]).
fn unresolved(spec: &Spec, machine: Option<&str>, text: &str, place: Place) -> Vec<(Unresolved, Option<String>)> {
    placeholder::placeholders(text, place)
        .into_iter()
        .filter_map(|found| {
            let (why, written) = match found {
                Ok(r) => {
                    let why = resolve(spec, machine, &r).err()?;
                    (format!("`{{{{ {r} }}}}`: {why}"), r.to_string())
                }
                Err(Error::Unclosed) => return Some((Unresolved::Unclosed(Error::Unclosed.to_string()), None)),
                Err(Error::Placeholder { path, why }) => (Error::Placeholder { path: path.clone(), why }.to_string(), path),
                Err(Error::NotHere { reference, why }) => {
                    let written = reference.to_string();
                    (Error::NotHere { reference, why }.to_string(), written)
                }
                Err(e) => return Some((Unresolved::Unknown(e.to_string()), None)),
            };
            Some((Unresolved::Unknown(why), nearest(spec, machine, &written, place)))
        })
        .collect()
}

/// The path nearest to `written` that `place` may name and the spec has, one segment at a time: a
/// typo in a machine's, a port's or a network's name, or in a field (`.uri`: `.url`). None when a
/// segment is near nothing there, or when the nearest doesn't resolve either (`.published` of a
/// port that isn't published).
fn nearest(spec: &Spec, template_of: Option<&str>, written: &str, place: Place) -> Option<String> {
    let roots: &[&str] = match place {
        Place::Message => &["machines"],
        Place::Template => &["machine", "machines"],
        Place::Entry | Place::EntryItem => return None,
    };
    let fields: &[&str] = if place == Place::Message { &["port", "published", "url"] } else { &["port"] };
    let parts: Vec<&str> = written.split('.').collect();
    let (root, rest) = parts.split_first()?;
    let root = closest(root, roots)?;
    let mut near = vec![root];
    match (root, rest) {
        ("machine", [field, rest @ ..]) => {
            let own = spec.machines.get(template_of?)?;
            let field = closest(field, &["name", "addresses"])?;
            near.push(field);
            match (field, rest) {
                ("name", []) => {}
                ("addresses", [n]) => near.push(closest(n, &own.networks.keys().map(String::as_str).collect::<Vec<_>>())?),
                _ => return None,
            }
        }
        ("machines", [m, kind, rest @ ..]) => {
            let m = closest(m, &spec.machines.keys().map(String::as_str).collect::<Vec<_>>())?;
            let machine = spec.machines.get(m)?;
            let kind = closest(kind, &["ports", "addresses"])?;
            near.extend([m, kind]);
            match (kind, rest) {
                ("addresses", [n]) => near.push(closest(n, &machine.networks.keys().map(String::as_str).collect::<Vec<_>>())?),
                ("ports", [port, field]) => {
                    near.push(closest(port, &machine.ports.keys().map(PortName::as_str).collect::<Vec<_>>())?);
                    near.push(closest(field, fields)?);
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    let near = near.join(".");
    let resolves = || {
        placeholder::placeholders(&format!("{{{{ {near} }}}}"), place)
            .iter()
            .all(|r| r.as_ref().is_ok_and(|r| resolve(spec, template_of, r).is_ok()))
    };
    (near != written && resolves()).then_some(near)
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
        // A machine's own address is a template's to name: elsewhere there is no machine to be
        // "own" to (an empty name said "no machine named ``" before).
        Reference::OwnAddress { network } => match template_of {
            Some(machine) => on(machine, network),
            None => Err(format!("`machine.addresses.{network}` names the machine a template is for; there is none here")),
        },
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
        // Each names what every machine has, or an entry's own value: listed, so a new kind of
        // reference is resolved here before it is accepted.
        Reference::MachineName
        | Reference::Version
        | Reference::Os(_)
        | Reference::Var(_)
        | Reference::Setting { .. }
        | Reference::Item(_)
        | Reference::ItemPath(_) => Ok(()),
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
