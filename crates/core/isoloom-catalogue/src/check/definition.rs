//! An entry's definition on its own: its name is its folder's, its default is one of its
//! releases, each image is for an OS a release runs on, each setting's fields fit its type,
//! each port's setting is a port, its files are its own, and the lock pins no release it lacks.

use crate::catalogue::Entry;
use crate::entry::{PortNumber, Setting, SettingType, Source};
use crate::error::Finding;

/// The licence line every file under `files/` carries.
const SPDX: &str = "SPDX-License-Identifier: Apache-2.0";

/// The files every entry has beside `files/`.
const OWN: [&str; 3] = ["entry.yml", "lock.yml", "contract.yml"];

/// Every rule `entry`'s definition breaks.
pub(super) fn check(entry: &Entry) -> Vec<Finding> {
    let def = entry.definition();
    let name = entry.name();
    let at = |field: &str| format!("{name}/entry.yml: {field}");
    let mut out = Vec::new();
    if def.description.trim().is_empty() {
        out.push(Finding::new(at("description"), "say what the service is, for the catalogue page"));
    }
    if def.releases.is_empty() {
        out.push(Finding::new(at("releases"), "list at least one release"));
    }
    for (i, r) in def.releases.iter().enumerate() {
        if def.releases.iter().take(i).any(|o| o.version == r.version) {
            out.push(Finding::new(at(&format!("releases[{i}]")), format!("release {} is listed twice", r.version)));
        }
        if r.os.is_empty() {
            out.push(Finding::new(at(&format!("releases[{i}].os")), "list the OSes it runs on"));
        }
    }
    if def.release(&def.default).is_none() {
        out.push(Finding::new(
            at("default"),
            format!("`{}` isn't one of the releases ({})", def.default, def.releases_list()),
        ));
    }
    for (os, image) in &def.images {
        if !def.releases.iter().any(|r| r.runs_on(*os)) {
            out.push(Finding::new(at(&format!("images.{os}")), "no release runs on this OS"));
        }
        if !image.contains("{{ version }}") {
            out.push(Finding::new(
                at(&format!("images.{os}")),
                "write `{{ version }}` where the release goes, so each release has its own image",
            ));
        }
    }
    for (setting, s) in &def.settings {
        out.extend(
            setting_rules(s)
                .into_iter()
                .map(|reason| Finding::new(at(&format!("settings.{setting}")), reason)),
        );
    }
    for (port, p) in &def.ports {
        if let PortNumber::Setting(s) = &p.number
            && def.settings.get(s).is_none_or(|s| s.kind != SettingType::Port)
        {
            out.push(Finding::new(at(&format!("ports.{port}.setting")), format!("`{s}` isn't a `port` setting")));
        }
    }
    for (key, d) in &def.daemons {
        out.extend(
            super::daemon_rules(d)
                .into_iter()
                .map(|reason| Finding::new(at(&format!("daemons.{key}")), reason)),
        );
    }
    out.extend(files(entry));
    out.extend(lock(entry));
    out
}

/// What setting `s`'s fields break: each field fits its type, a default fits the setting.
fn setting_rules(s: &Setting) -> Vec<String> {
    let mut out = Vec::new();
    let only = |field: &str, kinds: &str| format!("only {kinds} settings take `{field}`");
    if s.description.trim().is_empty() {
        out.push("say what it sets, in `description`".to_owned());
    }
    if s.required && s.default.is_some() {
        out.push("`required` and `default` contradict each other".to_owned());
    }
    if (s.pattern.is_some() || s.values.is_some()) && s.kind != SettingType::String {
        out.push(only("pattern` and `values", "`string`"));
    }
    if (s.minimum.is_some() || s.maximum.is_some()) && s.kind != SettingType::Integer {
        out.push(only("minimum` and `maximum", "`integer`"));
    }
    if s.owner.is_some() && s.kind != SettingType::Password {
        out.push(only("owner", "`password`"));
    }
    match (s.kind == SettingType::List, s.items.is_some(), s.max.is_some()) {
        (true, false, _) => out.push("a `list` says its items' fields, in `items`".to_owned()),
        (false, true, _) | (false, _, true) => out.push(only("items` and `max", "`list`")),
        _ => {}
    }
    for (field, item) in s.items.iter().flatten() {
        if item.kind == SettingType::List {
            out.push(format!("item field `{field}`: a list's items don't nest lists"));
        }
        out.extend(setting_rules(item).into_iter().map(|r| format!("item field `{field}`: {r}")));
    }
    out
}

/// The entry's own files: only its three and `files/`, each of those Apache-2.0.
fn files(entry: &Entry) -> Vec<Finding> {
    let mut out = Vec::new();
    for (path, text) in entry.files() {
        let at = format!("{}/{path}", entry.name());
        if path.starts_with("files/") {
            if !text.contains(SPDX) {
                out.push(Finding::new(at, format!("an entry's file is Isoloom's own: carry `{SPDX}`")));
            }
        } else if !OWN.contains(&path.as_str()) {
            out.push(Finding::new(
                at,
                "an entry holds entry.yml, lock.yml, contract.yml and files/ only: nothing third-party",
            ));
        }
    }
    out
}

/// What the lock pins that the entry doesn't have, and a `distro` release it pins packages of.
fn lock(entry: &Entry) -> Vec<Finding> {
    let def = entry.definition();
    let at = format!("{}/lock.yml", entry.name());
    let mut out = Vec::new();
    for (version, locked) in &entry.lock().releases {
        match def.release(version) {
            None => out.push(Finding::new(
                format!("{at}: releases.{version}"),
                "no such release in entry.yml: run `cargo xtask catalogue refresh`",
            )),
            Some(r) if r.source == Source::Distro && !locked.packages.is_empty() => {
                out.push(Finding::new(
                    format!("{at}: releases.{version}.packages"),
                    "a `distro` release follows its OS's archive: it pins no package version",
                ));
            }
            Some(_) => {}
        }
    }
    out
}
