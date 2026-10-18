//! `tiers.json` read into the typed list: each entry's target and provider or cloud known, each
//! run's host known (so its OS is), each date a calendar date. Parsed once, here; the rest of the
//! crate holds only the typed entries.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use isoloom_manifest::HostOs;
use isoloom_spec::model::Target;
use serde::Deserialize;

use super::{Run, Tier, TierEntry, variants};
use crate::{Error, Result};

/// The file: the test hosts by name, then the entries.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Each host that runs real runs, and its OS.
    hosts: BTreeMap<String, HostOs>,
    /// The entries, in the order targets are listed.
    entries: Vec<EntryFile>,
}

/// An entry as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryFile {
    /// `docker`, `vagrant/libvirt`, `cloud-vm/aws`.
    entry: String,
    /// The tier 1.0 aims for.
    goal: u8,
    /// The host OSes it exists on.
    exists_on: BTreeSet<HostOs>,
    /// Whether it is experimental.
    #[serde(default)]
    experimental: bool,
    /// Its real runs.
    runs: Vec<RunFile>,
}

/// A run as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunFile {
    host: String,
    date: String,
    result: RunResult,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// Whether a real run passed: only a whole cycle that passed proves a host OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunResult {
    /// The whole cycle ran (up, its checks, down) and passed.
    Pass,
    /// Part of the cycle ran and passed; its note says which part, and what is still to run.
    Partial,
    /// It didn't pass.
    Fail,
}

impl RunResult {
    /// The result as `tiers.json` writes it: `pass`, `partial` or `fail`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            RunResult::Pass => "pass",
            RunResult::Partial => "partial",
            RunResult::Fail => "fail",
        }
    }
}

/// A calendar day, `YYYY-MM-DD`; days order as their text does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(String);

impl Date {
    /// The day written as `YYYY-MM-DD`, if it is one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Date> {
        let mut parts = text.split('-');
        let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
        let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
        let in_range = |s: &str, max: u8| s.parse::<u8>().is_ok_and(|v| (1..=max).contains(&v));
        (parts.next().is_none() && digits(y, 4) && digits(m, 2) && digits(d, 2) && in_range(m, 12) && in_range(d, 31)).then(|| Date(text.to_owned()))
    }

    /// The day as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The entries of `json`, written as `tiers.json` is.
pub(super) fn parse(json: &str) -> Result<Vec<TierEntry>> {
    let file: File = serde_json::from_str(json).map_err(Error::TiersJson)?;
    file.entries.into_iter().map(|e| entry(&file.hosts, e)).collect()
}

/// An entry as typed: its target and provider or cloud known, its runs' hosts known.
fn entry(hosts: &BTreeMap<String, HostOs>, e: EntryFile) -> Result<TierEntry> {
    let invalid = |what: String| Error::Tiers { entry: e.entry.clone(), what };
    let (target, variant) = named(&e.entry).ok_or_else(|| invalid("no such target, provider or cloud".to_owned()))?;
    let goal = match e.goal {
        1 => Tier::One,
        2 => Tier::Two,
        3 => Tier::Three,
        other => return Err(invalid(format!("goal {other}: a tier is 1, 2 or 3"))),
    };
    if e.exists_on.is_empty() {
        return Err(invalid("`exists_on` names no host OS".to_owned()));
    }
    let runs = e.runs.into_iter().map(|r| run(hosts, r).map_err(invalid)).collect::<Result<Vec<Run>>>()?;
    if let Some(r) = runs.iter().find(|r| !e.exists_on.contains(&r.os)) {
        return Err(invalid(format!("a run on {} ({}), where it doesn't exist", super::os_id(r.os), r.host)));
    }
    Ok(TierEntry {
        target,
        variant,
        goal,
        exists_on: e.exists_on,
        experimental: e.experimental,
        runs,
    })
}

/// A run as typed, or what is wrong with it.
fn run(hosts: &BTreeMap<String, HostOs>, r: RunFile) -> std::result::Result<Run, String> {
    let os = *hosts
        .get(&r.host)
        .ok_or_else(|| format!("a run on host `{}`, which `hosts` doesn't name", r.host))?;
    let date = Date::parse(&r.date).ok_or_else(|| format!("`{}` isn't a date (YYYY-MM-DD)", r.date))?;
    Ok(Run {
        host: r.host,
        os,
        date,
        result: r.result,
        server: r.server,
        note: r.note,
    })
}

/// The target and provider or cloud an entry names (`vagrant/libvirt`), when both exist.
fn named(entry: &str) -> Option<(Target, Option<&'static str>)> {
    let (target, variant) = match entry.split_once('/') {
        Some((t, v)) => (t, Some(v)),
        None => (entry, None),
    };
    let target = Target::ALL.into_iter().find(|t| t.id() == target)?;
    let known = variants(target);
    match variant {
        None if known.is_empty() => Some((target, None)),
        Some(v) => known.into_iter().find(|k| *k == v).map(|k| (target, Some(k))),
        None => None,
    }
}
