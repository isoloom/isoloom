//! An entry's parity contract, `catalogue/<entry>/contract.yml`, written by hand (design §4.6):
//! the cases a conformance lab is built for, the checks both forms must pass, and the facts
//! whose output must be equal in both. `cargo xtask catalogue check` runs it.

use std::borrow::Cow;

use indexmap::IndexMap;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

use crate::names::CaseName;
use crate::version::Version;

/// The parity contract.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    /// The set-ups a conformance lab is built for, by name; `default` always runs.
    pub cases: IndexMap<CaseName, Case>,
    /// What both forms must pass, in the spec's check grammar.
    pub checks: Vec<ContractCheck>,
    /// Commands whose output must be equal in both forms.
    #[serde(default)]
    pub facts: Vec<Fact>,
}

/// One set-up of the conformance lab.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// The service machine's `with:`.
    #[serde(default)]
    pub with: IndexMap<String, serde_json::Value>,
    /// The lab's project files, by path: their content, inline.
    #[serde(default)]
    pub project: IndexMap<String, String>,
    /// Helper machines, by name, as a spec writes a machine (`{ service: php-fpm, with: … }`):
    /// each gets the service machine's OS and network.
    #[serde(default)]
    pub machines: IndexMap<String, serde_json::Value>,
}

/// One check of the contract: a check of the spec's grammar (`http`, `tcp`, `exec`, `script`,
/// `expect`, `contains`, `wait`), with `{{ machine }}`, `{{ version }}` and `{{ with.* }}` in its
/// texts. An `exec` runs in the service machine.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractCheck {
    /// The case it is limited to; every case when `None`.
    pub case: Option<CaseName>,
    /// The check as written, without `case`.
    pub check: serde_json::Value,
}

impl<'de> Deserialize<'de> for ContractCheck {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ContractCheck, D::Error> {
        let mut check = serde_json::Value::deserialize(d)?;
        let case = match check.as_object_mut() {
            Some(map) => map
                .shift_remove("case")
                .map(serde_json::from_value::<CaseName>)
                .transpose()
                .map_err(serde::de::Error::custom)?,
            None => return Err(serde::de::Error::custom("a contract check is a map: `{ name, http, expect }`")),
        };
        Ok(ContractCheck { case, check })
    }
}

impl JsonSchema for ContractCheck {
    fn schema_name() -> Cow<'static, str> {
        "ContractCheck".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "description": "A check of the spec's grammar (`name`, `http`, `tcp`, `exec`, `script`, `expect`, `contains`, `wait`), with `{{ machine }}`, `{{ version }}` and `{{ with.* }}`; `case` limits it to one case. An `exec` runs in the service machine.",
            "properties": { "case": { "type": "string" }, "name": { "type": "string" } },
            "required": ["name"],
            "additionalProperties": true
        })
    }
}

impl ContractCheck {
    /// Its name, as results show it.
    #[must_use]
    pub fn name(&self) -> &str {
        self.check.get("name").and_then(serde_json::Value::as_str).unwrap_or_default()
    }

    /// Whether it runs in case `case`.
    #[must_use]
    pub fn runs_in(&self, case: &CaseName) -> bool {
        self.case.as_ref().is_none_or(|c| c == case)
    }
}

/// The values a contract's placeholders take in one conformance run.
#[derive(Debug, Clone, Copy)]
pub struct CheckValues<'a> {
    /// The service machine's name: `{{ machine }}`.
    pub machine: &'a str,
    /// The release: `{{ version }}`.
    pub version: &'a Version,
    /// The case's settings, as text (its `with:`, else each default): `{{ with.<setting> }}`.
    pub with: &'a IndexMap<String, String>,
}

impl ContractCheck {
    /// The check with its placeholders filled in, read in the spec's grammar: a declared check
    /// with its name and exactly one of `http`, `tcp`, `exec` and `script`.
    ///
    /// # Errors
    ///
    /// A placeholder that names nothing here, or a check the spec's grammar doesn't read.
    pub fn filled(&self, values: &CheckValues<'_>) -> Result<isoloom_spec::model::Declared, String> {
        let filled = fill_json(&self.check, values)?;
        let check: isoloom_spec::model::Check = serde_json::from_value(filled).map_err(|e| format!("not a check of the spec's grammar: {e}"))?;
        let isoloom_spec::model::Check::Declared(d) = check else {
            return Err("a contract check is a map with its `name`".to_owned());
        };
        let kinds = [d.http.is_some(), d.tcp.is_some(), d.exec.is_some(), d.script.is_some()]
            .into_iter()
            .filter(|k| *k)
            .count();
        if d.name.is_none() {
            return Err("name the check: results show it".to_owned());
        }
        if kinds != 1 {
            return Err("a check has exactly one of `http`, `tcp`, `exec` and `script`".to_owned());
        }
        Ok(*d)
    }
}

/// `v` with every text's placeholders filled in.
fn fill_json(v: &serde_json::Value, values: &CheckValues<'_>) -> Result<serde_json::Value, String> {
    use serde_json::Value;
    Ok(match v {
        Value::String(s) => Value::String(fill(s, values)?),
        Value::Array(items) => Value::Array(items.iter().map(|i| fill_json(i, values)).collect::<Result<_, _>>()?),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, field)| Ok((k.clone(), fill_json(field, values)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

/// `text` with its `{{ machine }}`, `{{ version }}` and `{{ with.<setting> }}` filled in.
///
/// # Errors
///
/// A placeholder that names anything else, or a setting the case leaves without a value.
pub fn fill(text: &str, values: &CheckValues<'_>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{{") {
        out.push_str(before);
        let (inner, next) = after.split_once("}}").ok_or_else(|| format!("`{text}`: a `{{{{` without its `}}}}`"))?;
        let name = inner.trim();
        let value = match name.split_once('.') {
            None if name == "machine" => values.machine.to_owned(),
            None if name == "version" => values.version.to_string(),
            Some(("with", setting)) => values
                .with
                .get(setting)
                .cloned()
                .ok_or_else(|| format!("`{{{{ {name} }}}}`: the case gives `{setting}` no value"))?,
            _ => return Err(format!("`{{{{ {name} }}}}`: a contract names `machine`, `version` and `with.<setting>`")),
        };
        out.push_str(&value);
        rest = next;
    }
    out.push_str(rest);
    Ok(out)
}

/// A command run in the service machine, whose output must be the same in both forms.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    /// What it states (`the OS`).
    pub name: String,
    /// The command, run by `sh -c`.
    pub exec: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_keeps_its_case_apart() {
        let c: Contract = serde_yaml_ng::from_str(
            "cases: { default: {}, site: { with: { root: site/ }, project: { site/index.html: ok } } }\nchecks:\n  - { name: a, http: 'http://{{ machine }}/', case: site }\n  - { name: b, exec: nginx -v }\nfacts: [{ name: os, exec: cat /etc/os-release }]\n",
        )
        .unwrap();
        let site: CaseName = "site".parse().unwrap();
        let default: CaseName = "default".parse().unwrap();
        assert!(c.checks[0].runs_in(&site) && !c.checks[0].runs_in(&default) && c.checks[1].runs_in(&default));
        assert_eq!(c.checks[0].check, serde_json::json!({ "name": "a", "http": "http://{{ machine }}/" }));
        assert_eq!(c.checks[1].name(), "b");
    }
}
