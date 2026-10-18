//! One case of an entry's contract as a lab: the service machine (`web`), the case's helper
//! machines, and an access machine the runner supplies (`probe`), on one network with the
//! internet (the recipe fetches its packages); the contract's checks, filled in for the case, run
//! from `probe` (an `exec`, in `web`); each fact as a check of its own.

use indexmap::IndexMap;
use isoloom_spec::syntax::OsRef;
use serde_json::{Map, Value, json};

use crate::catalogue::{Catalogue, Entry};
use crate::contract::CheckValues;
use crate::entry::Release;
use crate::names::CaseName;

/// The service machine's name: `{{ machine }}` in the contract's checks.
pub const MACHINE: &str = "web";
/// The access machine the checks run from.
pub const PROBE: &str = "probe";
/// What a fact's check is named after: `fact: <name>`.
pub const FACT_PREFIX: &str = "fact: ";
/// What a fact's check expects, which its output never holds: the check always reports its
/// output.
const NEVER: &str = "isoloom-fact-output-follows";

/// One case of the contract, as a lab.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConformanceLab {
    /// Its `isoloom.yml`.
    pub spec: String,
    /// Its project files, by path, with their content.
    pub project: IndexMap<String, String>,
    /// The contract's checks the case runs, by name.
    pub checks: Vec<String>,
    /// The contract's facts, by name.
    pub facts: Vec<String>,
}

/// Why a case can't be run as a lab.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LabError {
    /// The contract has no such case.
    #[error("the contract has no case `{0}`")]
    NoCase(String),
    /// A helper machine runs a service the catalogue doesn't have yet.
    #[error("case `{case}`: its helper `{machine}` runs `{service}`, which the catalogue doesn't have yet")]
    MissingEntry {
        /// The case.
        case: String,
        /// The helper machine.
        machine: String,
        /// The service it names.
        service: String,
    },
    /// A check doesn't fill in for the case.
    #[error("case `{case}`: {reason}")]
    Check {
        /// The case.
        case: String,
        /// Why.
        reason: String,
    },
}

impl Entry {
    /// Case `case` of the contract as a lab of `release` on `os`, its helpers' services looked up
    /// in `catalogue`.
    ///
    /// # Errors
    ///
    /// [`LabError`]: an unknown case, a helper running a service the catalogue lacks, a check
    /// that doesn't fill in.
    pub fn conformance_lab(&self, catalogue: &Catalogue, case: &CaseName, release: &Release, os: OsRef) -> Result<ConformanceLab, LabError> {
        let c = self.contract().cases.get(case).ok_or_else(|| LabError::NoCase(case.to_string()))?;
        let mut machines = Map::new();
        machines.insert(
            MACHINE.to_owned(),
            json!({ "os": os.to_string(), "service": format!("{}@{}", self.name(), release.version), "with": c.with, "networks": { "lab": 10 } }),
        );
        for (i, (name, written)) in c.machines.iter().enumerate() {
            let service = written.get("service").and_then(Value::as_str).unwrap_or_default();
            let entry = service.split('@').next().unwrap_or_default();
            if !entry.is_empty() && catalogue.entry(entry).is_none() {
                return Err(LabError::MissingEntry {
                    case: case.to_string(),
                    machine: name.clone(),
                    service: service.to_owned(),
                });
            }
            let mut helper = written.as_object().cloned().unwrap_or_default();
            helper.entry("os".to_owned()).or_insert_with(|| json!(os.to_string()));
            helper.insert("networks".to_owned(), json!({ "lab": 20 + i }));
            machines.insert(name.clone(), Value::Object(helper));
        }
        machines.insert(PROBE.to_owned(), json!({ "access": true, "networks": { "lab": 200 } }));
        let with = self.check_values(&c.with);
        let values = CheckValues {
            machine: MACHINE,
            version: &release.version,
            with: &with,
        };
        let mut checks = Vec::new();
        let mut names = Vec::new();
        for check in self.contract().checks.iter().filter(|k| k.runs_in(case)) {
            let mut filled = check.filled(&values).map_err(|reason| LabError::Check {
                case: case.to_string(),
                reason,
            })?;
            filled.from = Some(if filled.exec.is_some() { MACHINE } else { PROBE }.to_owned());
            names.push(filled.name.clone().unwrap_or_default());
            checks.push(without_nulls(serde_json::to_value(&filled).unwrap_or_default()));
        }
        let facts: Vec<String> = self.contract().facts.iter().map(|f| f.name.clone()).collect();
        checks.extend(self.contract().facts.iter().map(fact_check));
        let spec = json!({
            "version": 1,
            "name": format!("conformance-{}-{case}", self.name()),
            "networks": { "lab": { "cidr": "10.77.0.0/24", "internet": true } },
            "machines": machines,
            "checks": checks,
        });
        Ok(ConformanceLab {
            spec: serde_yaml_ng::to_string(&spec).unwrap_or_default(),
            project: c.project.clone(),
            checks: names,
            facts,
        })
    }

    /// The text of each setting in a case: its `with:` value, else its default.
    fn check_values(&self, with: &IndexMap<String, Value>) -> IndexMap<String, String> {
        let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
        self.definition()
            .settings
            .iter()
            .filter_map(|(name, s)| {
                let v = with.get(name.as_str()).or(s.default.as_ref())?;
                Some((name.to_string(), text(v)))
            })
            .collect()
    }
}

/// `v` without its fields that hold nothing.
fn without_nulls(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(map.into_iter().filter(|(_, field)| !field.is_null()).collect()),
        other => other,
    }
}

/// Fact `f` as a check: its command in the service machine, its output folded into one line,
/// which the check's failure reports.
fn fact_check(f: &crate::contract::Fact) -> Value {
    json!({
        "name": format!("{FACT_PREFIX}{}", f.name),
        "from": MACHINE,
        "exec": format!("( {} ) 2>&1 | tr '\\n' ' '", f.exec),
        "expect": NEVER,
    })
}
