//! An entry's contract names only what the entry has: a `default` case, cases whose `with:`
//! keys are settings, checks limited to cases that exist, and each check a check of the spec's
//! grammar once its placeholders are filled in.

use indexmap::IndexMap;

use crate::catalogue::Entry;
use crate::contract::{CheckValues, ContractCheck};
use crate::error::Finding;

/// Every rule `entry`'s contract breaks.
pub(super) fn check(entry: &Entry) -> Vec<Finding> {
    let contract = entry.contract();
    let settings = &entry.definition().settings;
    let at = |field: &str| format!("{}/contract.yml: {field}", entry.name());
    let mut out = Vec::new();
    if !contract.cases.keys().any(|c| c.as_str() == "default") {
        out.push(Finding::new(at("cases"), "a `default: {}` case always runs: add it"));
    }
    for (case, c) in &contract.cases {
        for key in c.with.keys().filter(|k| !settings.keys().any(|s| s.as_str() == k.as_str())) {
            out.push(Finding::new(
                at(&format!("cases.{case}.with.{key}")),
                format!("{} has no setting `{key}`", entry.name()),
            ));
        }
    }
    for (i, check) in contract.checks.iter().enumerate() {
        if let Some(case) = &check.case
            && !contract.cases.contains_key(case)
        {
            out.push(Finding::new(at(&format!("checks[{i}].case")), format!("no case named `{case}`")));
        }
        if let Err(reason) = grammar(entry, check) {
            out.push(Finding::new(at(&format!("checks[{i}]")), reason));
        }
    }
    for (i, fact) in contract.facts.iter().enumerate() {
        if fact.exec.trim().is_empty() {
            out.push(Finding::new(
                at(&format!("facts[{i}].exec")),
                "a fact is a command whose output both forms share",
            ));
        }
    }
    out
}

/// Whether `check`, filled in with samples, is a check of the spec's grammar.
fn grammar(entry: &Entry, check: &ContractCheck) -> Result<(), String> {
    let def = entry.definition();
    let with: IndexMap<String, String> = def.settings.keys().map(|k| (k.to_string(), "sample".to_owned())).collect();
    let values = CheckValues {
        machine: "web",
        version: &def.default,
        with: &with,
    };
    check.filled(&values).map(|_| ())
}
