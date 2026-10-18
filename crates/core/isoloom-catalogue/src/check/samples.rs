//! The recipe resolves everywhere it claims to: on each release and each of its OSes, with the
//! settings left to their defaults, with each optional one set in turn, and with all of them
//! set, every step fills in, pins from `lock.yml` and parses. Each run resolves a small lab of
//! its own: the service machine, a peer for endpoints, a secret and an input.

use indexmap::IndexMap;
use isoloom_spec::syntax::{self, OsRef};
use serde_json::{Value, json};

use crate::catalogue::{Catalogue, Entry};
use crate::entry::{Release, Setting, SettingType};
use crate::error::Finding;
use crate::names::SettingName;

/// The machine the samples resolve, and its peer.
const MACHINE: &str = "web";
const PEER: &str = "peer";

/// Every place `entry`'s recipe doesn't resolve.
pub(super) fn check(c: &Catalogue, entry: &Entry) -> Vec<Finding> {
    let def = entry.definition();
    let mut out = Vec::new();
    for release in &def.releases {
        for os in &release.os {
            for (label, with) in variants(&def.settings) {
                if let Err(reason) = resolve(c, entry, release, *os, &with) {
                    out.push(Finding::new(
                        format!("{}/entry.yml: recipe", entry.name()),
                        format!("{} on {os}, {label}: {reason}", release.version),
                    ));
                }
            }
        }
    }
    out
}

/// Resolves `entry`'s `release` on `os` with `with`: the problems' reasons when it fails.
fn resolve(c: &Catalogue, entry: &Entry, release: &Release, os: OsRef, with: &IndexMap<String, Value>) -> Result<(), String> {
    let yaml = lab(entry, release, os, with);
    let spec = syntax::parse(&yaml).map_err(|p| reasons(&p))?;
    c.resolve(&spec, MACHINE).map(|_| ()).map_err(|p| reasons(&p))
}

/// Problems, as one line.
fn reasons(problems: &[syntax::Problem]) -> String {
    problems.iter().map(|p| format!("{}: {}", p.path, p.reason)).collect::<Vec<_>>().join("; ")
}

/// A lab of the service machine and its peer, written as a spec.
fn lab(entry: &Entry, release: &Release, os: OsRef, with: &IndexMap<String, Value>) -> String {
    let with = serde_json::to_string(with).unwrap_or_default();
    format!(
        "version: 1\nname: entry-check\nnetworks: {{ lab: {{ cidr: 10.99.0.0/24 }} }}\nsecrets: {{ sample: {{ env: SAMPLE }} }}\ninputs: [SAMPLE]\nmachines:\n  {MACHINE}: {{ os: {os}, service: {}@{}, with: {with}, networks: {{ lab: 10 }} }}\n  {PEER}: {{ os: {os}, networks: {{ lab: 11 }} }}\n",
        entry.name(),
        release.version
    )
}

/// The settings to resolve with, each with what it is: the defaults (and a sample of each
/// required setting), then each optional setting set in turn, then all of them set.
fn variants(settings: &IndexMap<SettingName, Setting>) -> Vec<(String, IndexMap<String, Value>)> {
    let base: IndexMap<String, Value> = settings
        .iter()
        .filter(|(_, s)| s.required)
        .filter_map(|(n, s)| Some((n.to_string(), sample(s)?)))
        .collect();
    let mut out = vec![("the defaults".to_owned(), base.clone())];
    let mut all = base.clone();
    for (name, s) in settings.iter().filter(|(_, s)| !s.required) {
        let Some(v) = sample(s) else { continue };
        let mut one = base.clone();
        one.insert(name.to_string(), v.clone());
        all.insert(name.to_string(), v);
        out.push((format!("`{name}` set"), one));
    }
    out.push(("every setting set".to_owned(), all));
    out
}

/// A value of setting `s`'s type that `s` accepts, when one can be made up.
fn sample(s: &Setting) -> Option<Value> {
    Some(match s.kind {
        SettingType::String => match (&s.values, &s.pattern) {
            (Some(values), _) => json!(values.first()?),
            (None, Some(p)) if !p.matches("sample") => return s.default.clone(),
            (None, _) => json!("sample"),
        },
        SettingType::Integer => json!(s.minimum.or(s.maximum).unwrap_or(1)),
        SettingType::Boolean => json!(true),
        SettingType::Port => json!(8080),
        SettingType::Endpoint => json!(format!("{PEER}:9000")),
        SettingType::Folder => json!("sample/"),
        SettingType::File => json!("sample/file.conf"),
        SettingType::Password => json!({ "secret": "sample" }),
        SettingType::List => {
            let item: serde_json::Map<String, Value> = s.items.iter().flatten().filter_map(|(n, f)| Some((n.to_string(), sample(f)?))).collect();
            json!([item])
        }
    })
}
