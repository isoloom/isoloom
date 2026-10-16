//! A machine's `with:` read against its entry's settings (design §4.4): every value typed, the
//! defaults filled in, and every problem placed at its key with its fix. What a value becomes in
//! the recipe's texts is its placeholder's text ([`SettingValue::text`]); a password reaches the
//! machine as a value file, by its path only ([`SettingValue::path`]).

mod read;

pub(crate) use read::{Around, read};

use std::net::IpAddr;
use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::syntax::{InputName, ProjectPath};

use crate::family::Family;
use crate::names::SettingName;

/// A machine's settings, typed: what `with:` gives, and every default it leaves.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    values: IndexMap<SettingName, SettingValue>,
}

/// One setting's value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettingValue {
    /// A `string`.
    Text(String),
    /// An `integer`.
    Integer(i64),
    /// A `boolean`.
    Boolean(bool),
    /// A `port`.
    Port(NonZeroU16),
    /// An `endpoint`.
    Endpoint(Endpoint),
    /// A `folder` of the project.
    Folder(ProjectPath),
    /// A `file` of the project.
    File(ProjectPath),
    /// A `password`, and where its value file is.
    Password {
        /// Where the value comes from.
        value: Password,
        /// Its value file in the machine.
        path: String,
    },
    /// A `list`: its items, each its fields' values.
    List(Vec<IndexMap<SettingName, SettingValue>>),
}

/// Where an `endpoint` setting points.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Endpoint {
    /// A machine of the lab, on the first network it shares with this one.
    Machine {
        /// The machine.
        machine: String,
        /// The network they share.
        network: String,
        /// The port.
        port: NonZeroU16,
    },
    /// An address.
    Address {
        /// The address.
        address: IpAddr,
        /// The port.
        port: NonZeroU16,
    },
}

/// Where a `password` setting's value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Password {
    /// Written in the spec: lab content, which lands in generated files.
    Literal(String),
    /// A secret of the spec, read from its store at each start.
    Secret(String),
    /// An input, given at each start.
    Input(InputName),
}

/// Why a setting's value can't stand in a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotText {
    /// A secret's or an input's value: only its file's path is ever written.
    Sensitive,
    /// A list: a step reads its items with `for_each`.
    List,
    /// Not a password, so it has no value file.
    NoFile,
}

impl Settings {
    /// The value of setting `name`, when `with:` gives it or it has a default.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SettingValue> {
        self.values.get(name)
    }

    /// Whether setting `name` has a value (given, or its default).
    #[must_use]
    pub fn is_set(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The values, in the entry's order of its settings.
    pub fn iter(&self) -> impl Iterator<Item = (&SettingName, &SettingValue)> {
        self.values.iter()
    }
}

impl SettingValue {
    /// What `{{ with.<setting> }}` (or `{{ item.<field> }}`) writes: the value as text; a
    /// folder's path without its trailing `/`; a machine endpoint as a placeholder of the
    /// machine's address, filled in for each target.
    ///
    /// # Errors
    ///
    /// [`NotText`]: a secret's or an input's value, or a list.
    pub fn text(&self) -> Result<String, NotText> {
        Ok(match self {
            SettingValue::Text(t)
            | SettingValue::Password {
                value: Password::Literal(t), ..
            } => t.clone(),
            SettingValue::Integer(i) => i.to_string(),
            SettingValue::Boolean(b) => b.to_string(),
            SettingValue::Port(p) => p.to_string(),
            SettingValue::Endpoint(Endpoint::Machine { machine, network, port }) => format!("{{{{ machines.{machine}.addresses.{network} }}}}:{port}"),
            SettingValue::Endpoint(Endpoint::Address { address: IpAddr::V6(a), port }) => format!("[{a}]:{port}"),
            SettingValue::Endpoint(Endpoint::Address { address, port }) => format!("{address}:{port}"),
            SettingValue::Folder(p) => p.as_str().trim_end_matches(['/', '\\']).to_owned(),
            SettingValue::File(p) => p.as_str().to_owned(),
            SettingValue::Password { .. } => return Err(NotText::Sensitive),
            SettingValue::List(_) => return Err(NotText::List),
        })
    }

    /// What `{{ with.<setting>.path }}` writes: a password's value file.
    ///
    /// # Errors
    ///
    /// [`NotText::NoFile`] for a value that isn't a password.
    pub fn path(&self) -> Result<&str, NotText> {
        match self {
            SettingValue::Password { path, .. } => Ok(path),
            _ => Err(NotText::NoFile),
        }
    }

    /// The value as JSON, where a step's field takes a number or a boolean (`priority: "{{
    /// with.priority }}"`), else its text.
    ///
    /// # Errors
    ///
    /// [`NotText`], as [`SettingValue::text`].
    pub fn json(&self) -> Result<serde_json::Value, NotText> {
        match self {
            SettingValue::Integer(i) => Ok((*i).into()),
            SettingValue::Port(p) => Ok(p.get().into()),
            SettingValue::Boolean(b) => Ok((*b).into()),
            other => other.text().map(serde_json::Value::from),
        }
    }
}

/// Where the value file of password setting `setting` goes on `family` (`field`, `index`: a
/// list item's field, the item from 1).
#[must_use]
pub fn value_file(family: Family, setting: &SettingName, item: Option<(usize, &SettingName)>) -> String {
    let (root, sep) = if family == Family::Windows {
        ("C:\\isoloom\\with", "\\")
    } else {
        ("/run/isoloom/with", "/")
    };
    match item {
        Some((index, field)) => format!("{root}{sep}{setting}{sep}{index}{sep}{field}"),
        None => format!("{root}{sep}{setting}"),
    }
}

#[cfg(test)]
mod tests;
