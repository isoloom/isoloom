//! Every setting does something, in every form (design §4.4): the recipe, the image or a port
//! uses it, and, when the entry has an official image (which skips `install`), something besides
//! `install` does. A string setting that reaches a template's file keeps to a `pattern` or
//! `values`, so nothing can be injected through it. A template's file is the entry's own; a copy
//! places the project's content a setting names.

use std::collections::BTreeSet;

use isoloom_spec::syntax::placeholder::{self, Place, Reference};
use serde_json::Value;

use crate::catalogue::Entry;
use crate::entry::{EntryStep, PortNumber, SettingType};
use crate::error::Finding;

/// Every rule `entry`'s use of its settings and files breaks.
pub(super) fn check(entry: &Entry) -> Vec<Finding> {
    let def = entry.definition();
    let at = |field: &str| format!("{}/entry.yml: {field}", entry.name());
    let mut out = Vec::new();
    let mut install = BTreeSet::new();
    let mut elsewhere = BTreeSet::new();
    for (phase, steps) in def.recipe.phases() {
        let used = if phase == "install" { &mut install } else { &mut elsewhere };
        for (i, step) in steps.iter().enumerate() {
            uses_of_step(entry, step, used);
            out.extend(
                sources(entry, step)
                    .into_iter()
                    .map(|reason| Finding::new(at(&format!("recipe.{phase}[{i}]")), reason)),
            );
        }
    }
    if let Some(image) = &def.image {
        let texts = image.environment.values().chain(image.files.values()).chain(image.command.iter().flatten());
        texts.for_each(|t| settings_in(t, &mut elsewhere));
    }
    for p in def.ports.values() {
        if let PortNumber::Setting(s) = &p.number {
            elsewhere.insert(s.to_string());
        }
    }
    for (name, setting) in &def.settings {
        let name = name.as_str();
        if !install.contains(name) && !elsewhere.contains(name) {
            out.push(Finding::new(
                at(&format!("settings.{name}")),
                "no step, image field or port uses it: it would do nothing",
            ));
        } else if !def.images.is_empty() && !elsewhere.contains(name) {
            out.push(Finding::new(
                at(&format!("settings.{name}")),
                "only `install` uses it, which the official image skips: use it in `configure`, `start` or `image` too",
            ));
        }
        if setting.kind == SettingType::String && setting.pattern.is_none() && setting.values.is_none() && in_templates(entry, name) {
            out.push(Finding::new(
                at(&format!("settings.{name}")),
                "a string that reaches a template's file keeps to a `pattern` or `values`, so nothing is injected through it",
            ));
        }
    }
    out
}

/// The settings `step` uses: its texts, its template's file, its `when` and `for_each`.
fn uses_of_step(entry: &Entry, step: &EntryStep, used: &mut BTreeSet<String>) {
    for t in strings(&step.body) {
        settings_in(t, used);
    }
    if let Some(text) = template(step).and_then(|from| entry.file(from)) {
        settings_in(text, used);
    }
    let when = step.when.as_ref();
    used.extend(
        when.and_then(|w| w.set.as_ref())
            .into_iter()
            .chain(when.and_then(|w| w.unset.as_ref()))
            .chain(step.for_each.as_ref())
            .map(ToString::to_string),
    );
}

/// Every text in `v`, its keys aside.
fn strings(v: &Value) -> Vec<&str> {
    match v {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) => items.iter().flat_map(strings).collect(),
        Value::Object(map) => map.values().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

/// The settings `text`'s placeholders name.
fn settings_in(text: &str, used: &mut BTreeSet<String>) {
    for r in placeholder::placeholders(text, Place::EntryItem).into_iter().flatten() {
        if let Reference::Setting { name, .. } = r {
            used.insert(name);
        }
    }
}

/// The file a template step places, as written.
fn template(step: &EntryStep) -> Option<&str> {
    step.body.pointer("/template/from").and_then(Value::as_str)
}

/// Whether a template's file of the entry names setting `name`.
fn in_templates(entry: &Entry, name: &str) -> bool {
    let recipe = &entry.definition().recipe;
    recipe
        .phases()
        .iter()
        .flat_map(|(_, steps)| steps.iter())
        .filter_map(template)
        .filter_map(|f| entry.file(f))
        .any(|text| {
            let mut used = BTreeSet::new();
            settings_in(text, &mut used);
            used.contains(name)
        })
}

/// What `step`'s sources break: a template's file is one of the entry's `files/`; a copy places
/// the project's content, through a setting.
fn sources(entry: &Entry, step: &EntryStep) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(from) = template(step) {
        if !from.starts_with("files/") || from.contains("{{") {
            out.push(format!(
                "a template's `from` is a file of the entry, under `files/` (`files/site.conf`), not `{from}`"
            ));
        } else if entry.file(from).is_none() {
            out.push(format!("the template `{from}` isn't a file of the entry"));
        }
    }
    if let Some(from) = step.body.pointer("/copy/from").and_then(Value::as_str)
        && !from.trim_start().starts_with("{{ with.")
    {
        out.push(format!(
            "a copy places the project's content a setting names (`{{{{ with.root }}}}/`), not `{from}`: the entry's own files go through `template`"
        ));
    }
    out
}
