//! The catalogue: every entry (its definition, pins, contract and files), the daemons no entry
//! owns, and the pinned Galaxy collections, read from the files of `catalogue/` and checked
//! ([`crate::check`]), so a broken entry fails this crate's tests, never a user's spec.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use indexmap::IndexMap;
use isoloom_spec::syntax::UnitName;

use crate::collections::Collections;
use crate::contract::Contract;
use crate::embedded::FILES;
use crate::entry::{Daemon, Definition};
use crate::error::{Error, Result};
use crate::lock::Lock;
use crate::names::EntryName;

/// One entry, loaded: its definition, its pins, its contract and its own files.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    definition: Definition,
    lock: Lock,
    contract: Contract,
    files: BTreeMap<String, String>,
}

impl Entry {
    /// Its name (`nginx`).
    #[must_use]
    pub fn name(&self) -> &EntryName {
        &self.definition.name
    }

    /// What `entry.yml` says.
    #[must_use]
    pub fn definition(&self) -> &Definition {
        &self.definition
    }

    /// What `lock.yml` pins.
    #[must_use]
    pub fn lock(&self) -> &Lock {
        &self.lock
    }

    /// What `contract.yml` says.
    #[must_use]
    pub fn contract(&self) -> &Contract {
        &self.contract
    }

    /// A file of the entry, by its path in the entry's folder (`files/default.conf`).
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// Every file of the entry, by its path in the entry's folder.
    #[must_use]
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }
}

/// The whole catalogue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalogue {
    entries: IndexMap<EntryName, Entry>,
    daemons: IndexMap<UnitName, Daemon>,
    collections: Collections,
}

/// The files at the catalogue's top: the daemons no entry owns, and the pinned collections.
const DAEMONS: &str = "daemons.yml";
const COLLECTIONS: &str = "collections.yml";

impl Catalogue {
    /// The catalogue built into Isoloom (`catalogue/`, embedded when the crate is built), loaded
    /// and checked once.
    ///
    /// # Errors
    ///
    /// Why it doesn't load: this crate's tests fail first, so a released Isoloom never has one.
    pub fn embedded() -> std::result::Result<&'static Catalogue, &'static Error> {
        static CATALOGUE: OnceLock<Result<Catalogue>> = OnceLock::new();
        CATALOGUE.get_or_init(|| Catalogue::from_files(FILES.iter().copied())).as_ref()
    }

    /// A catalogue read from its files, by their path in the catalogue's folder
    /// (`nginx/entry.yml`), then checked.
    ///
    /// # Errors
    ///
    /// [`Error::Yaml`] for a file its type can't read, [`Error::Missing`] for an entry without
    /// one of its three files, [`Error::Invalid`] with every rule the entries break.
    pub fn from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Catalogue> {
        let mut by_entry: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
        let mut catalogue = Catalogue::default();
        for (path, text) in files {
            match path.split_once('/') {
                Some((dir, rest)) => {
                    by_entry.entry(dir).or_default().insert(rest.to_owned(), text.to_owned());
                }
                None if path == DAEMONS => catalogue.daemons = parse(path, text)?,
                None if path == COLLECTIONS => catalogue.collections = parse(path, text)?,
                None => {}
            }
        }
        for (dir, own) in by_entry {
            let read = |file: &'static str| own.get(file).ok_or(Error::Missing { entry: dir.to_owned(), file });
            let entry = Entry {
                definition: parse(&format!("{dir}/entry.yml"), read("entry.yml")?)?,
                lock: parse(&format!("{dir}/lock.yml"), read("lock.yml")?)?,
                contract: parse(&format!("{dir}/contract.yml"), read("contract.yml")?)?,
                files: own.clone(),
            };
            let name = dir.parse::<EntryName>()?;
            catalogue.entries.insert(name, entry);
        }
        let findings = crate::check::catalogue(&catalogue);
        if findings.is_empty() { Ok(catalogue) } else { Err(Error::Invalid(findings)) }
    }

    /// The entry named `name`.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|(n, _)| n.as_str() == name).map(|(_, e)| e)
    }

    /// The entries, in name order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// The entries' names, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.entries.keys().map(EntryName::as_str).collect()
    }

    /// The daemon table: every entry's daemons, then those of `daemons.yml`, by the key a
    /// `service` step names.
    pub fn daemons(&self) -> impl Iterator<Item = (&UnitName, &Daemon)> {
        self.entries.values().flat_map(|e| e.definition.daemons.iter()).chain(self.daemons.iter())
    }

    /// The daemons no entry owns (`daemons.yml`).
    #[must_use]
    pub fn shared_daemons(&self) -> &IndexMap<UnitName, Daemon> {
        &self.daemons
    }

    /// The pinned Galaxy collections (`collections.yml`).
    #[must_use]
    pub fn collections(&self) -> &Collections {
        &self.collections
    }
}

/// A YAML file of the catalogue, as type `T`.
fn parse<T: serde::de::DeserializeOwned>(path: &str, text: &str) -> Result<T> {
    serde_yaml_ng::from_str(text).map_err(|source| Error::Yaml { path: path.to_owned(), source })
}
