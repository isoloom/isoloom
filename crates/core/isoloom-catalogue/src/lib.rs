//! Isoloom's service catalogue: what a machine's `service: nginx@1.30` with its `with:` means on
//! its OS, in both forms (design `plan/phase4/design.md` §4).
//!
//! - **Data, embedded:** the entries are the repository's `catalogue/<entry>/` folders
//!   (`entry.yml` by hand, `lock.yml` from `cargo xtask catalogue refresh`, `contract.yml`,
//!   `files/`), embedded when the crate is built ([`Catalogue::embedded`]), so generating never
//!   reads the disk or the network for them.
//! - **Checked when loaded** ([`Catalogue::from_files`]): a broken entry fails this crate's
//!   tests, never a user's spec. Each recipe is resolved on every release and OS it claims.
//! - **Resolved for a machine** ([`Catalogue::resolve`]): the entry, the release its version
//!   picks ([`pick`]: `postgres@16` is the newest 16.x), the OS, `with:` typed against the
//!   settings ([`Settings`]), and the recipe with `when:` and `for_each:` applied, placeholders
//!   filled in and `lock.yml`'s pins written in ([`Resolved`]). Lowering (`isoloom-plan`) builds
//!   both forms from it.
//! - **The daemon table** ([`Catalogue::daemons`]): each daemon's unit and package per
//!   [`Family`], and its foreground form for containers.
//! - **Schemas** ([`schema`]): of `entry.yml`, `contract.yml`, each entry's `with:`, and the
//!   spec's schema with each entry's `with:` rule.
//!
//! It sits above the spec (`isoloom-spec`) and below the plan, and is a functional core: no
//! I/O, clock or environment, the embedded files aside.
//!
//! # Example
//!
//! ```
//! use isoloom_catalogue::Catalogue;
//!
//! let catalogue = Catalogue::embedded().map_err(|e| e.to_string())?;
//! let spec = isoloom_spec::syntax::parse(
//!     "version: 1
//! name: shop
//! networks: { lab: { cidr: 10.9.0.0/24 } }
//! machines:
//!   web: { os: debian@12, service: nginx@1.30, with: { port: 8080 }, networks: { lab: 10 } }
//! ",
//! )
//! .map_err(|p| format!("{p:?}"))?;
//! let resolved = catalogue.resolve(&spec, "web").map_err(|p| format!("{p:?}"))?;
//! assert_eq!(resolved[0].release.as_str(), "1.30.6");
//! assert_eq!(resolved[0].ports.values().next().map(|p| p.port.get()), Some(8080));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod catalogue;
mod check;
mod collections;
pub mod conformance;
mod contract;
mod embedded;
mod entry;
mod error;
mod family;
mod fetches;
mod lock;
mod names;
mod resolve;
pub mod schema;
mod settings;
mod version;

pub use catalogue::{Catalogue, Entry};
pub use collections::{Collection, Collections};
pub use contract::{Case, CheckValues, Contract, ContractCheck, Fact, fill as fill_contract};
pub use entry::{
    Daemon, Definition, EntryPort, EntryStep, ImageStart, ImageUse, Licence, LogStream, OsFamily, Pattern, PerFamily, PortNumber, Recipe, Release, Setting,
    SettingType, Source, When,
};
pub use error::{Error, Finding, Result};
pub use family::{Family, codename};
pub use fetches::{Fetches, KeyCheck, RepositoryFetch};
pub use lock::{Digest, Lock, LockedImage, LockedKey, LockedRelease};
pub use names::{CaseName, DataName, EntryName, Kind, Name, SettingName, VarName};
pub use resolve::{OfficialImage, Resolved, ResolvedPort, ValueFile};
pub use settings::{Endpoint, NotText, Password, SettingValue, Settings, value_file};
pub use version::{Version, pick};
