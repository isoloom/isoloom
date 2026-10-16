//! Reading `with:` against the settings (design §4.9, step 4): an unknown key (with a
//! did-you-mean), a wrong type, a missing required setting, a secret or an input where the
//! setting isn't a password, and an endpoint whose machine shares no network with this one are
//! each a problem at the key.

use std::net::IpAddr;
use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::syntax::{self, Code, FieldPath, InputName, Problem, Problems, ProjectPath, closest};
use serde_json::Value;

use super::{Endpoint, Password, SettingValue, Settings, value_file};
use crate::entry::{Setting, SettingType};
use crate::family::Family;
use crate::names::SettingName;

/// What the values may name: the lab, the machine they are for and its OS's family.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Around<'a> {
    /// The lab.
    pub spec: &'a syntax::Spec,
    /// The machine.
    pub machine: &'a str,
    /// Its OS's family (where value files go).
    pub family: Family,
}

/// `with` read against `settings` (`entry`: its name, for messages; `at`: `with`'s path).
///
/// # Errors
///
/// Every problem, each at its key.
pub(crate) fn read(
    entry: &str,
    settings: &IndexMap<SettingName, Setting>,
    with: &IndexMap<String, Value>,
    around: Around<'_>,
    at: &FieldPath,
) -> Result<Settings, Problems> {
    let mut problems = Vec::new();
    let known: Vec<&str> = settings.keys().map(SettingName::as_str).collect();
    for key in with.keys().filter(|k| !known.contains(&k.as_str())) {
        problems.push(unknown(entry, key, &known, &at.key(key)));
    }
    let mut values = IndexMap::new();
    for (name, setting) in settings {
        let path = at.key(name.as_str());
        let given = with.get(name.as_str()).or(setting.default.as_ref());
        match given {
            Some(v) => match value(
                setting,
                v,
                &Place {
                    around,
                    setting: name,
                    item: None,
                    path: &path,
                },
            ) {
                Ok(typed) => {
                    values.insert(name.clone(), typed);
                }
                Err(p) => problems.extend(p),
            },
            None if setting.required => problems.push(
                Problem::new(
                    Code::MissingField,
                    path,
                    format!("{entry} needs `{name}`: {}", first_sentence(&setting.description)),
                )
                .fix(format!("`with: {{ {name}: <{}> }}`", setting.kind.id())),
            ),
            None => {}
        }
    }
    if problems.is_empty() { Ok(Settings { values }) } else { Err(problems) }
}

/// A key the settings don't have.
fn unknown(entry: &str, key: &str, known: &[&str], at: &FieldPath) -> Problem {
    let p = Problem::new(Code::UnknownField, at.clone(), format!("{entry} has no setting `{key}`"));
    match closest(key, known) {
        Some(k) => p.fix(format!("did you mean `{k}`?")),
        None if known.is_empty() => p.fix(format!("{entry} takes no settings: leave `with` out")),
        None => p.fix(format!(
            "its settings: {}",
            known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The first sentence of a description, for a message.
fn first_sentence(text: &str) -> &str {
    text.split_once(". ").map_or(text, |(s, _)| s).trim_end_matches('.')
}

/// Where a value is read: the lab, the setting, the list item (its index from 1 and field) and
/// the value's path.
struct Place<'a> {
    around: Around<'a>,
    setting: &'a SettingName,
    item: Option<(usize, &'a SettingName)>,
    path: &'a FieldPath,
}

/// `v` typed as `setting` says.
fn value(setting: &Setting, v: &Value, at: &Place<'_>) -> Result<SettingValue, Problems> {
    if setting.kind != SettingType::Password && reference(v).is_some() {
        return Err(vec![
            Problem::new(
                Code::InvalidValue,
                at.path.clone(),
                format!(
                    "`{}` is {}: only a `password` setting takes a secret or an input",
                    at.setting,
                    setting.kind.expected()
                ),
            )
            .fix("write the value in the spec"),
        ]);
    }
    let wrong = || {
        vec![Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{}` is {}", at.setting, setting.kind.expected()),
        )]
    };
    match (setting.kind, v) {
        (SettingType::String, Value::String(t)) => text(setting, t, at).map_err(|p| vec![p]),
        (SettingType::Integer, Value::Number(n)) => n.as_i64().ok_or_else(wrong).and_then(|i| integer(setting, i, at).map_err(|p| vec![p])),
        (SettingType::Boolean, Value::Bool(b)) => Ok(SettingValue::Boolean(*b)),
        (SettingType::Port, Value::Number(n)) => n
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .and_then(NonZeroU16::new)
            .map(SettingValue::Port)
            .ok_or_else(wrong),
        (SettingType::Endpoint, Value::String(t)) => endpoint(t, at).map(SettingValue::Endpoint).map_err(|p| vec![p]),
        (SettingType::Folder, Value::String(t)) => t
            .parse::<ProjectPath>()
            .map(SettingValue::Folder)
            .map_err(|e| vec![invalid(at, format!("`{}` is {}: {e}", at.setting, setting.kind.expected()))]),
        (SettingType::File, Value::String(t)) => t
            .parse::<ProjectPath>()
            .map(SettingValue::File)
            .map_err(|e| vec![invalid(at, format!("`{}` is {}: {e}", at.setting, setting.kind.expected()))]),
        (SettingType::Password, v) => password(v, at).map_err(|p| vec![p]),
        (SettingType::List, Value::Array(items)) => list(setting, items, at),
        _ => Err(wrong()),
    }
}

/// A value's error, at its path.
fn invalid(at: &Place<'_>, e: impl std::fmt::Display) -> Problem {
    Problem::new(Code::InvalidValue, at.path.clone(), e.to_string())
}

/// `{ secret: <name> }` or `{ input: <NAME> }`: its kind and name.
fn reference(v: &Value) -> Option<(&str, &str)> {
    let map = v.as_object().filter(|m| m.len() == 1)?;
    let (key, name) = map.iter().next()?;
    matches!(key.as_str(), "secret" | "input").then_some((key.as_str(), name.as_str()?))
}

/// A `string` that keeps to its `values` or `pattern`.
fn text(setting: &Setting, t: &str, at: &Place<'_>) -> Result<SettingValue, Problem> {
    if let Some(values) = &setting.values
        && !values.iter().any(|v| v == t)
    {
        let list = values.iter().map(|v| format!("`{v}`")).collect::<Vec<_>>().join(", ");
        return Err(Problem::new(Code::InvalidValue, at.path.clone(), format!("`{t}` isn't one of `{}`'s values", at.setting)).fix(format!("one of {list}")));
    }
    if let Some(p) = &setting.pattern
        && !p.matches(t)
    {
        return Err(Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{t}` doesn't match `{}`'s pattern `{}`", at.setting, p.as_str()),
        ));
    }
    Ok(SettingValue::Text(t.to_owned()))
}

/// An `integer` within its bounds.
fn integer(setting: &Setting, i: i64, at: &Place<'_>) -> Result<SettingValue, Problem> {
    let below = setting.minimum.is_some_and(|m| i < m);
    let above = setting.maximum.is_some_and(|m| i > m);
    if below || above {
        let bounds = match (setting.minimum, setting.maximum) {
            (Some(lo), Some(hi)) => format!("from {lo} to {hi}"),
            (Some(lo), None) => format!("at least {lo}"),
            (None, Some(hi)) => format!("at most {hi}"),
            (None, None) => String::new(),
        };
        return Err(Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{}` is {bounds}, not {i}", at.setting),
        ));
    }
    Ok(SettingValue::Integer(i))
}

/// An `endpoint`: `<machine>:<port>`, the machine sharing a network with this one, or
/// `<address>:<port>`.
fn endpoint(t: &str, at: &Place<'_>) -> Result<Endpoint, Problem> {
    let wrong = || {
        Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{t}` isn't `<machine>:<port>` or `<address>:<port>`"),
        )
        .fix("`app:9000`")
    };
    let (host, port) = t.rsplit_once(':').ok_or_else(wrong)?;
    let port = port.parse::<NonZeroU16>().map_err(|_| wrong())?;
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if let Ok(address) = bare.parse::<IpAddr>() {
        return Ok(Endpoint::Address { address, port });
    }
    let spec = at.around.spec;
    let Some(other) = spec.machines.get(host) else {
        let names: Vec<&str> = spec.machines.keys().map(String::as_str).collect();
        let p = Problem::new(Code::UnknownReference, at.path.clone(), format!("no machine named `{host}`"));
        return Err(match closest(host, &names) {
            Some(n) => p.fix(format!("did you mean `{n}`?")),
            None => p,
        });
    };
    let own = spec.machines.get(at.around.machine).map(|m| &m.networks);
    let network = own.into_iter().flat_map(IndexMap::keys).find(|n| other.networks.contains_key(*n));
    match network {
        Some(network) => Ok(Endpoint::Machine {
            machine: host.to_owned(),
            network: network.clone(),
            port,
        }),
        None => Err(Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{host}` shares no network with `{}`", at.around.machine),
        )
        .fix(format!("put both on one network, or name an address (`<address>:{port}`)"))),
    }
}

/// A `password`: a literal, or a secret or an input the spec declares.
fn password(v: &Value, at: &Place<'_>) -> Result<SettingValue, Problem> {
    let spec = at.around.spec;
    let value = match (v, reference(v)) {
        (Value::String(t), _) => Password::Literal(t.clone()),
        (_, Some(("secret", name))) if spec.secrets.contains_key(name) => Password::Secret(name.to_owned()),
        (_, Some(("secret", name))) => {
            return Err(Problem::new(Code::UnknownReference, at.path.key("secret"), format!("no secret named `{name}`"))
                .fix(format!("declare it under `secrets:` (`{name}: {{ env: <VAR> }}`)")));
        }
        (_, Some((_, name))) => match name.parse::<InputName>() {
            Ok(input) if spec.inputs.get(&input).is_some() => Password::Input(input),
            _ => {
                return Err(Problem::new(Code::UnknownReference, at.path.key("input"), format!("no input named `{name}`"))
                    .fix(format!("declare it under `inputs:` (`inputs: [{name}]`)")));
            }
        },
        _ => return Err(invalid(at, format!("`{}` is {}", at.setting, SettingType::Password.expected()))),
    };
    Ok(SettingValue::Password {
        value,
        path: value_file(at.around.family, at.setting, at.item),
    })
}

/// A `list`: each item a map of the setting's `items`, at most `max`.
fn list(setting: &Setting, items: &[Value], at: &Place<'_>) -> Result<SettingValue, Problems> {
    let fields = setting.items.clone().unwrap_or_default();
    let mut problems = Vec::new();
    if let Some(max) = setting.max
        && items.len() > usize::try_from(max).unwrap_or(usize::MAX)
    {
        problems.push(Problem::new(
            Code::InvalidValue,
            at.path.clone(),
            format!("`{}` takes at most {max} items, not {}", at.setting, items.len()),
        ));
    }
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = at.path.index(i);
        let Some(map) = item.as_object() else {
            problems.push(Problem::new(
                Code::InvalidValue,
                path,
                format!("an item of `{}` is a map of its fields", at.setting),
            ));
            continue;
        };
        let given: IndexMap<String, Value> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        match item_fields(&fields, &given, at, i + 1, &path) {
            Ok(values) => out.push(values),
            Err(p) => problems.extend(p),
        }
    }
    if problems.is_empty() { Ok(SettingValue::List(out)) } else { Err(problems) }
}

/// One list item's fields (`index`: the item, from 1).
fn item_fields(
    fields: &IndexMap<SettingName, Setting>,
    given: &IndexMap<String, Value>,
    at: &Place<'_>,
    index: usize,
    path: &FieldPath,
) -> Result<IndexMap<SettingName, SettingValue>, Problems> {
    let known: Vec<&str> = fields.keys().map(SettingName::as_str).collect();
    let mut problems: Problems = given
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .map(|k| unknown(&format!("an item of `{}`", at.setting), k, &known, &path.key(k)))
        .collect();
    let mut values = IndexMap::new();
    for (name, field) in fields {
        let field_path = path.key(name.as_str());
        let place = Place {
            around: at.around,
            setting: at.setting,
            item: Some((index, name)),
            path: &field_path,
        };
        match given.get(name.as_str()).or(field.default.as_ref()) {
            Some(v) => match value(field, v, &place) {
                Ok(typed) => {
                    values.insert(name.clone(), typed);
                }
                Err(p) => problems.extend(p),
            },
            None if field.required => problems.push(Problem::new(
                Code::MissingField,
                field_path,
                format!("an item of `{}` needs `{name}`", at.setting),
            )),
            None => {}
        }
    }
    if problems.is_empty() { Ok(values) } else { Err(problems) }
}

#[cfg(test)]
mod tests {
    use super::{FieldPath, unknown};

    #[test]
    fn an_unknown_setting_fix_depends_on_whether_the_entry_has_any() {
        let at = FieldPath::new("machines").key("web").key("with");
        let none = unknown("demo", "nope", &[], &at);
        assert_eq!(none.fix.as_deref(), Some("demo takes no settings: leave `with` out"));
        let some = unknown("demo", "zzzzz", &["port", "greeting"], &at);
        assert_eq!(some.fix.as_deref(), Some("its settings: `port`, `greeting`"));
    }
}
