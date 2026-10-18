//! What `cargo xtask catalogue refresh` fetches to pin an entry (design §4.8), worked out from
//! the definition alone, since the lock is what it writes: for one release on one OS, the
//! official image's tag, each repository the recipe adds with its key, and each package the
//! recipe pins to the release. A step any setting may enable counts: `when:`'s `set` and `unset`
//! are taken as holding, and a step whose texts need a setting's value is left out.

use isoloom_spec::syntax::{OsRef, PackageName, Sha256};
use serde_json::Value;

use crate::entry::{Definition, EntryStep, Release};
use crate::family::Family;
use crate::resolve::{Text, Values};
use crate::settings::Settings;

/// What the refresh fetches for one release on one OS.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Fetches {
    /// The official image's tag, when the entry has one for the OS.
    pub image: Option<String>,
    /// The repositories the recipe adds.
    pub repositories: Vec<RepositoryFetch>,
    /// The packages the recipe pins to the release (a `packages` item with a `version`).
    pub packages: Vec<PackageName>,
}

/// A repository the recipe adds, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RepositoryFetch {
    /// Its address, filled in.
    pub url: String,
    /// The package manager it is for: the OS's family.
    pub family: Family,
    /// apt's suite (the step's, else the OS's codename).
    pub suite: Option<String>,
    /// apt's components (the step's, else `main`).
    pub components: Vec<String>,
    /// Its key's address.
    pub key_url: String,
    /// How its key is checked.
    pub key: KeyCheck,
}

/// How a repository key is checked when it is fetched: the two ways an entry pins one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCheck {
    /// A GPG key: its file holds a primary key of this fingerprint (40 hex digits).
    Fingerprint(String),
    /// Any other key: its file's SHA-256.
    Sha256(Sha256),
}

impl Definition {
    /// What the refresh fetches for `release` on `os`.
    ///
    /// # Errors
    ///
    /// A repository step that doesn't read as one once filled in.
    pub fn fetches(&self, release: &Release, os: OsRef) -> Result<Fetches, String> {
        let settings = Settings::default();
        let values = Values {
            release,
            os: os.os(),
            settings: &settings,
            item: None,
        };
        let mut out = Fetches {
            image: self
                .images
                .get(&os)
                .map(|t| values.text(t, Text::Field).map_err(|e| e.to_string()))
                .transpose()?,
            ..Fetches::default()
        };
        let steps = self.recipe.phases().into_iter().flat_map(|(_, steps)| steps.iter());
        for step in steps.filter(|s| applies(s, release, os)) {
            // A step that needs a setting's value fetches nothing the refresh can know.
            let Ok(body) = values.json(&step.body) else { continue };
            if let Some(r) = body.get("repository") {
                out.repositories.push(repository(r, os)?);
            }
            for p in body.get("packages").and_then(Value::as_array).into_iter().flatten() {
                let name = p.get("name").and_then(Value::as_str).filter(|_| p.get("version").is_some());
                if let Some(name) = name.and_then(|n| n.parse::<PackageName>().ok())
                    && !out.packages.contains(&name)
                {
                    out.packages.push(name);
                }
            }
        }
        Ok(out)
    }
}

/// Whether `step` may apply to `release` on `os`, whatever the settings.
fn applies(step: &EntryStep, release: &Release, os: OsRef) -> bool {
    let Some(w) = &step.when else { return true };
    w.family.as_ref().is_none_or(|f| f.iter().any(|f| f.as_str() == os.os().family))
        && w.os.as_ref().is_none_or(|l| l.contains(&os))
        && w.release.as_ref().is_none_or(|l| l.iter().any(|word| release.is(word)))
}

/// A repository step's fetch, its suite and components as the plan fills them in.
fn repository(r: &Value, os: OsRef) -> Result<RepositoryFetch, String> {
    let text = |key: &str| r.get(key).and_then(Value::as_str).map(str::to_owned);
    let url = text("url").ok_or("a repository has its `url`")?;
    let key = r.get("key").ok_or("a repository has its `key`")?;
    let key_url = key.get("url").and_then(Value::as_str).ok_or("a repository's key has its `url`")?.to_owned();
    let check = match (key.get("fingerprint").and_then(Value::as_str), key.get("sha256").and_then(Value::as_str)) {
        (Some(f), None) => KeyCheck::Fingerprint(f.to_ascii_uppercase()),
        (None, Some(s)) => KeyCheck::Sha256(s.parse().map_err(|e: isoloom_spec::Error| e.to_string())?),
        _ => return Err(format!("the key `{key_url}` takes exactly one of `fingerprint` and `sha256`")),
    };
    let family = Family::from(os);
    let apt = family == Family::Debian;
    let suite = text("suite").or_else(|| crate::family::codename(os.os()).map(str::to_owned)).filter(|_| apt);
    let components = match r.get("components").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        None if apt => vec!["main".to_owned()],
        None => Vec::new(),
    };
    Ok(RepositoryFetch {
        url,
        family,
        suite,
        components,
        key_url,
        key: check,
    })
}
