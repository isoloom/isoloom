//! The one placeholder grammar of the whole spec: `{{ path }}`, in the message, in an app's
//! templates and in a catalogue entry's recipe and files. Isoloom fills every placeholder in when
//! it generates (never Jinja2 or envsubst), so every CM tool installs the same bytes.
//!
//! | Path | Where |
//! |---|---|
//! | `machine.name`, `machine.addresses.<network>` | templates (this machine's) |
//! | `machines.<m>.addresses.<network>`, `machines.<m>.ports.<name>.port` | templates and the message |
//! | `machines.<m>.ports.<name>.published`, `.url` | the message only: run-time values |
//! | `cloud.outputs.<name>` | the message |
//! | `version`, `os.family`, `os.version`, `os.codename`, `vars.<x>`, `with.<setting>`, `with.<setting>.path` | a catalogue entry |
//! | `item.<field>`, `item.<field>.path` | a catalogue entry's step repeated with `for_each` |
//!
//! Templates are logic-free: no conditionals, no loops. A secret's or an input's value is never
//! a placeholder; an entry reaches such a setting's file by its path (`with.<setting>.path`).

use std::fmt;
use std::str::FromStr;

use super::names::PortName;
use crate::Error;
use crate::placeholders::{self, Piece};

/// What a placeholder names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Reference {
    /// `machine.name`: the machine the template is for.
    MachineName,
    /// `machine.addresses.<network>`: its address on that network.
    OwnAddress {
        /// The network.
        network: String,
    },
    /// `machines.<m>.addresses.<network>`: another machine's address on a network.
    Address {
        /// The machine.
        machine: String,
        /// The network.
        network: String,
    },
    /// `machines.<m>.ports.<name>.port|published|url`: a machine's port.
    Port {
        /// The machine.
        machine: String,
        /// The port, by name.
        port: PortName,
        /// What of it.
        field: PortField,
    },
    /// `cloud.outputs.<name>`: an output of the cloud module.
    CloudOutput(String),
    /// `version`: a catalogue entry's resolved release.
    Version,
    /// `os.family`, `os.version`, `os.codename`: the machine's OS.
    Os(OsField),
    /// `vars.<x>`: a variable of the entry's release.
    Var(String),
    /// `with.<setting>`, or `with.<setting>.path` for a password given as a secret or an input.
    Setting {
        /// The setting.
        name: String,
        /// Its value, or its value file's path.
        field: SettingField,
    },
    /// `item.<field>`: a field of the list item a `for_each` step is repeated for.
    Item(String),
    /// `item.<field>.path`: the value file of a list item's password field.
    ItemPath(String),
}

/// What of a port a placeholder names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PortField {
    /// Its number in the machine.
    Port,
    /// The host port it is published on: known only once the lab runs.
    Published,
    /// Its URL (`http` and `https` ports): known only once the lab runs.
    Url,
}

/// What of the machine's OS a placeholder names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OsField {
    /// The family (`debian`).
    Family,
    /// The version (`12`).
    Version,
    /// The release's codename (`bookworm`).
    Codename,
}

/// What of a setting a placeholder names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SettingField {
    /// Its value.
    Value,
    /// Its value file's path: a password given as a secret or an input.
    Path,
}

/// Where a text with placeholders is written: each place has its own paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Place {
    /// The spec's `message`, shown once the lab is up.
    Message,
    /// An app's `template` step's file.
    Template,
    /// A catalogue entry's recipe or files.
    Entry,
    /// A catalogue entry's step repeated with `for_each`.
    EntryItem,
}

impl Reference {
    /// Whether its value is only known once the lab runs (a published port, a URL).
    #[must_use]
    pub fn is_run_time(&self) -> bool {
        matches!(
            self,
            Reference::Port {
                field: PortField::Published | PortField::Url,
                ..
            }
        )
    }

    /// Whether `place` may name it, else why not.
    ///
    /// # Errors
    ///
    /// Why the place can't name it: a run-time value in a file, an entry's path outside an entry.
    pub fn allowed_in(&self, place: Place) -> Result<(), &'static str> {
        let entry = matches!(place, Place::Entry | Place::EntryItem);
        match self {
            Reference::MachineName | Reference::OwnAddress { .. } if place == Place::Message => Err("the message names machines by `machines.<m>`"),
            Reference::Port { .. } if self.is_run_time() && place != Place::Message => {
                Err("a published port and a URL are known only once the lab runs; a file gets `port`")
            }
            Reference::CloudOutput(_) if place != Place::Message => Err("a cloud output is the message's only"),
            Reference::Version | Reference::Os(_) | Reference::Var(_) | Reference::Setting { .. } if !entry => Err("only a catalogue entry has it"),
            Reference::Item(_) | Reference::ItemPath(_) if place != Place::EntryItem => Err("only a step repeated with `for_each` has an `item`"),
            _ => Ok(()),
        }
    }
}

impl FromStr for Reference {
    type Err = Error;

    /// The reference a placeholder's path names (`machines.web.ports.site.port`).
    ///
    /// # Errors
    ///
    /// [`Error::Placeholder`] for a path the grammar doesn't have, with the paths it has there.
    fn from_str(path: &str) -> Result<Reference, Error> {
        let unknown = |why| Error::Placeholder { path: path.to_owned(), why };
        let parts: Vec<&str> = path.split('.').collect();
        match parts.as_slice() {
            ["machine", "name"] => Ok(Reference::MachineName),
            ["machine", "addresses", n] => Ok(Reference::OwnAddress { network: (*n).to_owned() }),
            ["machine", ..] => Err(unknown("this machine's paths are `machine.name` and `machine.addresses.<network>`")),
            ["machines", m, rest @ ..] => {
                machine(m, rest).ok_or_else(|| unknown("a machine's paths are `ports.<name>.port`, `.published`, `.url` and `addresses.<network>`"))
            }
            ["cloud", "outputs", name] => Ok(Reference::CloudOutput((*name).to_owned())),
            ["version"] => Ok(Reference::Version),
            ["os", "family"] => Ok(Reference::Os(OsField::Family)),
            ["os", "version"] => Ok(Reference::Os(OsField::Version)),
            ["os", "codename"] => Ok(Reference::Os(OsField::Codename)),
            ["os", ..] => Err(unknown("the OS's paths are `os.family`, `os.version` and `os.codename`")),
            ["vars", x] => Ok(Reference::Var((*x).to_owned())),
            ["with", s] => Ok(Reference::Setting {
                name: (*s).to_owned(),
                field: SettingField::Value,
            }),
            ["with", s, "path"] => Ok(Reference::Setting {
                name: (*s).to_owned(),
                field: SettingField::Path,
            }),
            ["item", field] => Ok(Reference::Item((*field).to_owned())),
            ["item", field, "path"] => Ok(Reference::ItemPath((*field).to_owned())),
            _ => Err(unknown(
                "the paths start with `machine.`, `machines.<m>.`, `cloud.outputs.`, `version`, `os.`, `vars.`, `with.` or `item.`",
            )),
        }
    }
}

/// `machines.<m>.<rest>`, when `rest` is one of a machine's paths.
fn machine(m: &str, rest: &[&str]) -> Option<Reference> {
    let machine = m.to_owned();
    match rest {
        ["addresses", n] => Some(Reference::Address {
            machine,
            network: (*n).to_owned(),
        }),
        ["ports", p, field] => {
            let field = match *field {
                "port" => PortField::Port,
                "published" => PortField::Published,
                "url" => PortField::Url,
                _ => return None,
            };
            Some(Reference::Port {
                machine,
                port: p.parse().ok()?,
                field,
            })
        }
        _ => None,
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::MachineName => f.write_str("machine.name"),
            Reference::OwnAddress { network } => write!(f, "machine.addresses.{network}"),
            Reference::Address { machine, network } => write!(f, "machines.{machine}.addresses.{network}"),
            Reference::Port { machine, port, field } => {
                let field = match field {
                    PortField::Port => "port",
                    PortField::Published => "published",
                    PortField::Url => "url",
                };
                write!(f, "machines.{machine}.ports.{port}.{field}")
            }
            Reference::CloudOutput(name) => write!(f, "cloud.outputs.{name}"),
            Reference::Version => f.write_str("version"),
            Reference::Os(OsField::Family) => f.write_str("os.family"),
            Reference::Os(OsField::Version) => f.write_str("os.version"),
            Reference::Os(OsField::Codename) => f.write_str("os.codename"),
            Reference::Var(x) => write!(f, "vars.{x}"),
            Reference::Setting {
                name,
                field: SettingField::Value,
            } => write!(f, "with.{name}"),
            Reference::Setting {
                name,
                field: SettingField::Path,
            } => write!(f, "with.{name}.path"),
            Reference::Item(field) => write!(f, "item.{field}"),
            Reference::ItemPath(field) => write!(f, "item.{field}.path"),
        }
    }
}

/// A piece of a text with placeholders, read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Part {
    /// Text, as written.
    Text(String),
    /// A placeholder.
    Placeholder(Reference),
}

/// The pieces of `text`, written at `place`: its text and its placeholders, in order.
///
/// # Errors
///
/// Every [`Error`] placeholder, in order: an unclosed one ends the text.
pub fn read(text: &str, place: Place) -> Result<Vec<Part>, Vec<Error>> {
    let mut parts = Vec::new();
    let mut wrong = Vec::new();
    for piece in placeholders::pieces(text) {
        match piece {
            Piece::Text(t) => parts.push(Part::Text(t.to_owned())),
            Piece::Placeholder(inner) => match reference(inner, place) {
                Ok(r) => parts.push(Part::Placeholder(r)),
                Err(e) => wrong.push(e),
            },
            Piece::Unclosed(_) => wrong.push(Error::Unclosed),
        }
    }
    if wrong.is_empty() { Ok(parts) } else { Err(wrong) }
}

/// `parts` joined back into a text, each placeholder replaced by what `value` gives it.
///
/// # Errors
///
/// The first error `value` gives.
pub fn join<E>(parts: Vec<Part>, mut value: impl FnMut(&Reference) -> Result<String, E>) -> Result<String, E> {
    let mut out = String::new();
    for part in parts {
        match part {
            Part::Text(t) => out.push_str(&t),
            Part::Placeholder(r) => out.push_str(&value(&r)?),
        }
    }
    Ok(out)
}

/// Each placeholder of `text`, written at `place`, in order: what it names, or why it can't be
/// read there.
#[must_use]
pub fn placeholders(text: &str, place: Place) -> Vec<Result<Reference, Error>> {
    placeholders::pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Text(_) => None,
            Piece::Placeholder(inner) => Some(reference(inner, place)),
            Piece::Unclosed(_) => Some(Err(Error::Unclosed)),
        })
        .collect()
}

/// What placeholder `inner` (between its braces) names, when `place` may name it.
fn reference(inner: &str, place: Place) -> Result<Reference, Error> {
    let r: Reference = inner.trim().parse()?;
    match r.allowed_in(place) {
        Ok(()) => Ok(r),
        Err(why) => Err(Error::NotHere { reference: r, why }),
    }
}

#[cfg(test)]
mod tests;
