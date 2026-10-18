//! An entry's placeholders filled in for one machine (design §1.4): `{{ version }}`, `{{ os.* }}`,
//! `{{ vars.* }}`, `{{ with.* }}` and, in a step repeated with `for_each`, `{{ item.* }}`. The
//! lab's own paths (`{{ machine.name }}`, `{{ machines.<m>.addresses.<n> }}`) are left as they
//! are in a template's text, for the plan to fill in per target; anywhere else they are refused,
//! since a step runs before the machine reaches the others.

use indexmap::IndexMap;
use isoloom_spec::os::Os;
use isoloom_spec::syntax::placeholder::{self, OsField, Part, Place, Reference, SettingField};
use serde_json::Value;

use crate::entry::Release;
use crate::family::codename;
use crate::names::SettingName;
use crate::settings::{NotText, SettingValue, Settings};

/// Why a placeholder can't be filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unfilled {
    /// It can't be read where it is written (the grammar's own reason).
    Unreadable(String),
    /// It names nothing the entry has, or a setting with no value here.
    Unknown(String),
    /// A secret's or an input's value: only its file's path is ever written.
    Sensitive(String),
    /// A value that can't stand in a text here (a list, a path of what has no file, a lab path
    /// outside a template).
    Misplaced { reference: String, why: &'static str },
}

impl std::fmt::Display for Unfilled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unfilled::Unreadable(why) => f.write_str(why),
            Unfilled::Unknown(r) => write!(
                f,
                "`{{{{ {r} }}}}` names nothing this entry gives here (a setting with no value needs `when: {{ set: … }}`)"
            ),
            Unfilled::Sensitive(r) => write!(f, "`{{{{ {r} }}}}` would write a secret's value: use its file, `{{{{ {r}.path }}}}`"),
            Unfilled::Misplaced { reference, why } => write!(f, "`{{{{ {reference} }}}}`: {why}"),
        }
    }
}

/// Where a text is: a template's file keeps the lab's paths; anything else refuses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Text {
    /// A template's file.
    Template,
    /// A step's field, the image's environment or command.
    Field,
}

/// The values an entry's placeholders take for one machine.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Values<'a> {
    /// The release resolved.
    pub release: &'a Release,
    /// The machine's OS.
    pub os: Os,
    /// The machine's settings.
    pub settings: &'a Settings,
    /// The list item of a step repeated with `for_each`.
    pub item: Option<&'a IndexMap<SettingName, SettingValue>>,
}

impl Values<'_> {
    /// Where the placeholders are read: an item's step has `item`.
    fn place(&self) -> Place {
        if self.item.is_some() { Place::EntryItem } else { Place::Entry }
    }

    /// `text` with its placeholders filled in.
    pub(crate) fn text(&self, text: &str, at: Text) -> Result<String, Unfilled> {
        let parts =
            placeholder::read(text, self.place()).map_err(|e| Unfilled::Unreadable(e.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")))?;
        let out = placeholder::join(parts, |r| self.reference(r, at))?;
        if at == Text::Field && out.contains("{{") {
            return Err(Unfilled::Misplaced {
                reference: text.to_owned(),
                why: "the lab's machines are named in a template's file only: a step runs before the machine reaches the others",
            });
        }
        Ok(out)
    }

    /// `v` with every text's placeholders filled in; a text that is one placeholder of a number or
    /// a boolean becomes that number or boolean.
    pub(crate) fn json(&self, v: &Value) -> Result<Value, Unfilled> {
        Ok(match v {
            Value::String(s) => match self.whole(s)? {
                Some(typed) => typed,
                None => Value::String(self.text(s, Text::Field)?),
            },
            Value::Array(items) => Value::Array(items.iter().map(|i| self.json(i)).collect::<Result<_, _>>()?),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, field)| Ok((k.clone(), self.json(field)?)))
                    .collect::<Result<_, Unfilled>>()?,
            ),
            other => other.clone(),
        })
    }

    /// The typed value of `s` when it is exactly one placeholder of a setting or item.
    fn whole(&self, s: &str) -> Result<Option<Value>, Unfilled> {
        let parts = placeholder::read(s, self.place());
        let Ok([Part::Placeholder(r)]) = parts.as_deref() else {
            return Ok(None);
        };
        match self.value_of(r)? {
            Some(v) => v.json().map(Some).map_err(|why| not_text(r, why)),
            None => Ok(None),
        }
    }

    /// The setting or item value `r` names, when it names one.
    fn value_of(&self, r: &Reference) -> Result<Option<&SettingValue>, Unfilled> {
        let found = match r {
            Reference::Setting {
                name,
                field: SettingField::Value,
            } => self.settings.get(name),
            Reference::Item(field) => self.item.and_then(|i| i.iter().find(|(k, _)| k.as_str() == field).map(|(_, v)| v)),
            _ => return Ok(None),
        };
        found.map(Some).ok_or_else(|| Unfilled::Unknown(r.to_string()))
    }

    /// What reference `r` writes.
    fn reference(&self, r: &Reference, at: Text) -> Result<String, Unfilled> {
        match r {
            Reference::Version => Ok(self.release.version.to_string()),
            Reference::Os(OsField::Family) => Ok(self.os.family.to_owned()),
            Reference::Os(OsField::Version) => Ok(self.os.version.to_owned()),
            Reference::Os(OsField::Codename) => codename(self.os).map(str::to_owned).ok_or_else(|| Unfilled::Misplaced {
                reference: r.to_string(),
                why: "the OS has no codename (only apt's releases have one)",
            }),
            Reference::Os(_) => Err(Unfilled::Unknown(r.to_string())),
            Reference::Var(x) => self
                .release
                .vars
                .iter()
                .find(|(k, _)| k.as_str() == x)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Unfilled::Unknown(r.to_string())),
            Reference::Setting {
                name,
                field: SettingField::Path,
            } => {
                let value = self.settings.get(name).ok_or_else(|| Unfilled::Unknown(r.to_string()))?;
                value.path().map(str::to_owned).map_err(|why| not_text(r, why))
            }
            Reference::ItemPath(field) => {
                let value = self.item.and_then(|i| i.iter().find(|(k, _)| k.as_str() == field).map(|(_, v)| v));
                value
                    .ok_or_else(|| Unfilled::Unknown(r.to_string()))?
                    .path()
                    .map(str::to_owned)
                    .map_err(|why| not_text(r, why))
            }
            Reference::Setting { .. } | Reference::Item(_) => match self.value_of(r)? {
                Some(v) => v.text().map_err(|why| not_text(r, why)),
                None => Err(Unfilled::Unknown(r.to_string())),
            },
            lab if at == Text::Template => Ok(format!("{{{{ {lab} }}}}")),
            lab => Err(Unfilled::Misplaced {
                reference: lab.to_string(),
                why: "the lab's machines are named in a template's file only: a step runs before the machine reaches the others",
            }),
        }
    }
}

/// Why `r`'s value can't stand in a text.
fn not_text(r: &Reference, why: NotText) -> Unfilled {
    match why {
        NotText::Sensitive => Unfilled::Sensitive(r.to_string()),
        NotText::List => Unfilled::Misplaced {
            reference: r.to_string(),
            why: "a list is read item by item, with `for_each`",
        },
        _ => Unfilled::Misplaced {
            reference: r.to_string(),
            why: "only a password has a value file",
        },
    }
}
