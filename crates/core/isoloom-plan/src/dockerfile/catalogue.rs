//! A base image that is a catalogue entry, resolved into the translation's steps: the entry's
//! recipe (install, configure, start) for the release and OS its `FROM` picked, with the settings
//! the image's environment gives. The image's environment fills a setting through the entry's own
//! `image.environment` read backwards (`MYSQL_DATABASE: "{{ with.database }}"` makes
//! `ENV MYSQL_DATABASE=app` the setting `database`), so a Dockerfile configures the VM the way it
//! configures the image. The entry's template files come along under `catalogue/<entry>/`, and a
//! password's value file is written from the value the environment gives.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use isoloom_catalogue::{Catalogue, Definition, Password, Resolved};
use isoloom_spec::syntax::{self, ProjectPath, Step};
use serde_json::{Value, json};

use super::Untranslatable;
use super::matrix::Fallback;
use super::translate::EntryOp;

/// An entry's part of the translation.
#[derive(Debug, Clone, Default)]
pub(super) struct Recipe {
    /// Its steps, in order: the value files, then install, configure and start.
    pub steps: Vec<Step>,
    /// The texts of the templates its steps place, by the path a step's `from` names.
    pub files: BTreeMap<ProjectPath, String>,
    /// The services its steps define or start: the VM's own when the image's command is the entry's.
    pub units: Vec<String>,
}

/// `op`'s recipe, its settings from `env` (the image's environment, for the last stage's entry;
/// `None`: the entry's defaults), for a Dockerfile named `file` in problems.
///
/// # Errors
///
/// [`Untranslatable`]: a required setting the environment doesn't give, or what the catalogue
/// refuses for the release, the OS or the settings.
pub(super) fn resolve(op: &EntryOp, env: Option<&[(String, String)]>, file: &str) -> Result<Recipe, Untranslatable> {
    let refused = |reason: String| Untranslatable::at(file, 0, "FROM", reason, "give the machine `os` and `app.steps`");
    let catalogue = Catalogue::embedded().map_err(|e| refused(format!("Isoloom's catalogue doesn't load: {e}")))?;
    let entry = catalogue
        .entry(op.call.entry)
        .ok_or_else(|| refused(format!("the catalogue has no `{}`", op.call.entry)))?;
    let with = settings(entry.definition(), env.unwrap_or_default(), op).map_err(refused)?;
    let yaml = format!(
        "version: 1\nname: dockerfile\nmachines:\n  m: {{ os: {}, service: {}@{}, with: {} }}\n",
        op.os,
        op.call.entry,
        op.release,
        Value::Object(with.into_iter().collect())
    );
    let spec = syntax::parse(&yaml).map_err(|p| refused(reasons(&p)))?;
    let resolved = catalogue.resolve(&spec, "m").map_err(|p| refused(reasons(&p)))?;
    let r = resolved
        .into_iter()
        .next()
        .ok_or_else(|| refused(format!("`{}@{}` resolves to nothing", op.call.entry, op.release)))?;
    recipe(op.call.entry, r).map_err(refused)
}

/// The settings `env` gives `def`'s entry, through its `image.environment` read backwards; a
/// required one it leaves out takes `op`'s fallback.
fn settings(def: &Definition, env: &[(String, String)], op: &EntryOp) -> Result<IndexMap<String, Value>, String> {
    let get = |name: &str| {
        let own = env.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        // The same variable under a prefix the image also reads (`MYSQL_` for `MARIADB_`).
        own.or_else(|| {
            op.call.aliases.iter().find_map(|(from, to)| {
                let alias = format!("{to}{}", name.strip_prefix(from)?);
                env.iter().rev().find(|(k, _)| *k == alias).map(|(_, v)| v.clone())
            })
        })
    };
    let mut with = IndexMap::new();
    for (var, template) in def.image.iter().flat_map(|i| i.environment.iter()) {
        let Some((setting, file)) = setting_of(template) else { continue };
        // `MYSQL_ROOT_PASSWORD_FILE: "{{ with.password.path }}"`: the image reads the value from
        // the variable without `_FILE` too, which is how a Dockerfile writes it.
        let plain = if file { var.strip_suffix("_FILE").unwrap_or(var) } else { var.as_str() };
        if let Some(value) = get(plain) {
            with.insert(setting, Value::String(value));
        }
    }
    for (name, setting) in &def.settings {
        let name = name.as_str();
        if !setting.required || with.contains_key(name) {
            continue;
        }
        let fallback = op.call.unset.iter().find(|(n, _)| *n == name).map(|(_, f)| *f);
        let value = match fallback {
            Some(Fallback::Value(v)) => Some(Value::String(v.to_owned())),
            Some(Fallback::Setting(other)) => with.get(other).cloned(),
            None => None,
        };
        if let Some(v) = value {
            with.insert(name.to_owned(), v);
            continue;
        }
        let var = def
            .image
            .iter()
            .flat_map(|i| i.environment.iter())
            .find(|(_, t)| setting_of(t).is_some_and(|(named, _)| named == name))
            .map_or_else(|| name.to_owned(), |(v, _)| v.trim_end_matches("_FILE").to_owned());
        return Err(format!(
            "the image's environment doesn't set `{var}`, which the catalogue's `{}` needs",
            op.call.entry
        ));
    }
    Ok(with)
}

/// The setting a `{{ with.<name> }}` (or `{{ with.<name>.path }}`, `true` then) names.
fn setting_of(template: &str) -> Option<(String, bool)> {
    let inner = template.trim().strip_prefix("{{")?.strip_suffix("}}")?.trim();
    let rest = inner.strip_prefix("with.")?;
    match rest.strip_suffix(".path") {
        Some(name) => Some((name.to_owned(), true)),
        None => Some((rest.to_owned(), false)),
    }
}

/// `r`'s steps for `entry`, its templates moved under `catalogue/<entry>/` and its literal
/// passwords written to their value files first.
fn recipe(entry: &str, r: Resolved) -> Result<Recipe, String> {
    let mut out = Recipe::default();
    let moved = |from: &ProjectPath| -> Result<ProjectPath, String> { format!("catalogue/{entry}/{from}").parse().map_err(|e| format!("{e}")) };
    for (i, vf) in r.value_files.iter().enumerate() {
        let Password::Literal(value) = &vf.value else {
            return Err("a password from a secret or an input has no value a Dockerfile's environment gives".to_owned());
        };
        let from: ProjectPath = format!("catalogue/{entry}/values/{i}").parse().map_err(|e| format!("{e}"))?;
        let mut t = json!({ "from": from.as_str(), "to": vf.path, "mode": "0600" });
        if let (Some(owner), Some(map)) = (&vf.owner, t.as_object_mut()) {
            map.insert("owner".to_owned(), json!(owner.as_str()));
        }
        out.steps.push(serde_json::from_value(json!({ "template": t })).map_err(|e| e.to_string())?);
        out.files.insert(from, value.clone());
    }
    for (from, text) in r.files {
        out.files.insert(moved(&from)?, text);
    }
    for mut step in r.install.into_iter().chain(r.configure).chain(r.start) {
        match &mut step {
            Step::Template(t) => t.from = moved(&t.from)?,
            Step::Service(s) => out.units.push(s.name().to_string()),
            _ => {}
        }
        out.steps.push(step);
    }
    Ok(out)
}

/// Problems, as one line.
fn reasons(problems: &[syntax::Problem]) -> String {
    problems.iter().map(|p| format!("{}: {}", p.path, p.reason)).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_read_from_its_template() {
        assert_eq!(setting_of("{{ with.database }}"), Some(("database".to_owned(), false)));
        assert_eq!(setting_of("{{ with.password.path }}"), Some(("password".to_owned(), true)));
        assert_eq!(setting_of("{{ version }}"), None);
    }
}
