//! An entry's recipe resolved for one machine (design §4.9, step 5): `when:` and `for_each:`
//! applied, placeholders filled in, `lock.yml`'s pins written in (a repository key's SHA-256, a
//! vendor package's exact version), and each template's file rendered; then each step is parsed
//! into the spec's [`Step`], so what comes out holds only valid values.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use isoloom_spec::syntax::{OsRef, ProjectPath, Step};
use serde_json::{Map, Value};

use super::fill::{Text, Values};
use crate::catalogue::Entry;
use crate::entry::{EntryStep, Release, Source, When};
use crate::names::SettingName;
use crate::settings::{SettingValue, Settings};

/// Where a recipe is resolved: the entry, its release, the machine's OS and settings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context<'a> {
    pub entry: &'a Entry,
    pub release: &'a Release,
    pub os: OsRef,
    pub settings: &'a Settings,
}

/// Why a step of the recipe can't be resolved: its phase, its index and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepError {
    pub phase: &'static str,
    pub index: usize,
    pub reason: String,
}

impl Context<'_> {
    /// Phase `phase`'s steps, resolved, its templates' texts added to `files`.
    pub(crate) fn phase(&self, phase: &'static str, steps: &[EntryStep], files: &mut BTreeMap<ProjectPath, String>) -> Result<Vec<Step>, StepError> {
        let mut out = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let fail = |reason: String| StepError { phase, index, reason };
            if !step.when.as_ref().is_none_or(|w| self.holds(w)) {
                continue;
            }
            for (n, item) in self.items(step).map_err(fail)?.into_iter().enumerate() {
                let values = Values {
                    release: self.release,
                    os: self.os.os(),
                    settings: self.settings,
                    item,
                };
                let body = values.json(&step.body).map_err(|e| fail(e.to_string()))?;
                let body = self.pinned(body).map_err(fail)?;
                let label = item.map(|_| n + 1);
                let body = self.rendered(body, &values, label, files).map_err(fail)?;
                out.push(serde_json::from_value::<Step>(body).map_err(|e| fail(e.to_string()))?);
            }
        }
        Ok(out)
    }

    /// Whether every condition of `w` holds here.
    pub(crate) fn holds(&self, w: &When) -> bool {
        let os = self.os.os();
        w.family.as_ref().is_none_or(|f| f.iter().any(|f| f.as_str() == os.family))
            && w.os.as_ref().is_none_or(|l| l.contains(&self.os))
            && w.release.as_ref().is_none_or(|l| l.iter().any(|word| self.release.is(word)))
            && w.set.as_ref().is_none_or(|s| self.settings.is_set(s.as_str()))
            && w.unset.as_ref().is_none_or(|s| !self.settings.is_set(s.as_str()))
    }

    /// The items `step` repeats for: one pass without an item, unless it has `for_each`.
    fn items(&self, step: &EntryStep) -> Result<Vec<Option<&IndexMap<SettingName, SettingValue>>>, String> {
        let Some(list) = &step.for_each else { return Ok(vec![None]) };
        match self.settings.get(list.as_str()) {
            Some(SettingValue::List(items)) => Ok(items.iter().map(Some).collect()),
            Some(_) => Err(format!("`for_each: {list}` names a setting that isn't a list")),
            None => Ok(Vec::new()),
        }
    }

    /// `body` with `lock.yml`'s pins: a repository key's SHA-256 for its fingerprint, and each
    /// package's exact version.
    fn pinned(&self, mut body: Value) -> Result<Value, String> {
        if let Some(key) = body.pointer_mut("/repository/key").and_then(Value::as_object_mut) {
            self.key(key)?;
        }
        if let Some(packages) = body.get_mut("packages").and_then(Value::as_array_mut) {
            for p in packages {
                self.package(p)?;
            }
        }
        Ok(body)
    }

    /// A key given by its fingerprint, its SHA-256 written from `lock.yml`.
    fn key(&self, key: &mut Map<String, Value>) -> Result<(), String> {
        let Some(fingerprint) = key.shift_remove("fingerprint") else { return Ok(()) };
        let url = key.get("url").and_then(Value::as_str).unwrap_or_default().to_owned();
        let locked = self.entry.lock().keys.iter().find(|(u, _)| u.as_str() == url).map(|(_, k)| k);
        match locked {
            Some(k) if k.fingerprint.as_deref() == fingerprint.as_str() => {
                key.insert("sha256".to_owned(), Value::String(k.sha256.to_string()));
                Ok(())
            }
            _ => Err(format!(
                "lock.yml pins no key `{url}` with fingerprint {fingerprint}: run `cargo xtask catalogue refresh {}`",
                self.entry.name()
            )),
        }
    }

    /// A package with its exact version from `lock.yml`; a vendor release's pinned package must
    /// be there.
    fn package(&self, p: &mut Value) -> Result<(), String> {
        let name = p.as_str().or_else(|| p.get("name").and_then(Value::as_str)).unwrap_or_default().to_owned();
        let pinned_here = p.get("version").is_some();
        let locked = name.parse().ok().and_then(|n| self.entry.lock().package(&self.release.version, self.os, &n));
        match locked {
            Some(v) => *p = serde_json::json!({ "name": name, "version": v.as_str() }),
            None if pinned_here && self.release.source == Source::Vendor => {
                return Err(format!(
                    "lock.yml pins no version of `{name}` for {} on {}: run `cargo xtask catalogue refresh {}`",
                    self.release.version,
                    self.os,
                    self.entry.name()
                ));
            }
            None => {}
        }
        Ok(())
    }

    /// `body` with its template's file rendered into `files`, under a path of its own that the
    /// step now names (`label`: the item's number, in a step repeated with `for_each`).
    fn rendered(&self, mut body: Value, values: &Values<'_>, label: Option<usize>, files: &mut BTreeMap<ProjectPath, String>) -> Result<Value, String> {
        let Some(from) = body.pointer_mut("/template/from") else { return Ok(body) };
        let file = from.as_str().unwrap_or_default().to_owned();
        let text = self
            .entry
            .file(&file)
            .ok_or_else(|| format!("the template `{file}` isn't a file of the entry (under `files/`)"))?;
        let text = values.text(text, Text::Template).map_err(|e| format!("{file}: {e}"))?;
        let item = label.map(|n| format!("item-{n}/")).unwrap_or_default();
        let key: ProjectPath = format!(".isoloom/catalogue/{}/{item}{file}", self.entry.name())
            .parse()
            .map_err(|e| format!("{file}: {e}"))?;
        *from = Value::String(key.to_string());
        files.insert(key, text);
        Ok(body)
    }
}
