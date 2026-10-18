//! An entry's definition, `catalogue/<entry>/entry.yml`, written by hand: what the service is,
//! its releases, its official images, the settings of `with:`, its ports, data, health and
//! daemons, how settings reach the official image, and its recipe (design §4.2). Data, not code:
//! a new entry needs no Rust. Its JSON Schema (`catalogue/entry.schema.json`) comes from these
//! types.

mod daemon;
mod release;
mod setting;
mod step;

pub use daemon::{Daemon, LogStream, PerFamily};
pub use release::{Release, Source};
pub use setting::{Pattern, Setting, SettingType};
pub use step::{EntryStep, OsFamily, Recipe, When};

use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::syntax::{AbsPath, Healthcheck, HttpsUrl, OsRef, PortName, PortProtocol, UnitName};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::names::{DataName, EntryName, SettingName};
use crate::version::Version;

/// One entry of the catalogue, as `entry.yml` writes it.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// The entry's name, its folder's, as a machine's `service:` writes it (`nginx`).
    pub name: EntryName,
    /// What it is, in a sentence or two, for the generated catalogue page.
    pub description: String,
    /// The software's home page.
    pub upstream: HttpsUrl,
    /// The software's licence, recorded for the catalogue page.
    pub licence: Licence,
    /// The releases a machine may ask for (`service: nginx@1.30`), each with its OSes.
    pub releases: Vec<Release>,
    /// The release `service: <name>` resolves to: one of `releases`, pinned, so a lab never
    /// changes under its author when a newer release is added.
    pub default: Version,
    /// The official images, by OS: an image reference with `{{ version }}`
    /// (`docker.io/library/nginx:{{ version }}-trixie`), only where the image's OS is that OS.
    #[serde(default)]
    pub images: IndexMap<OsRef, String>,
    /// The settings a machine gives in `with:`, by name: the schema of `with:`.
    #[serde(default)]
    pub settings: IndexMap<SettingName, Setting>,
    /// The machine's ports, by name: a fixed number, or a `port` setting's.
    #[serde(default)]
    pub ports: IndexMap<PortName, EntryPort>,
    /// Data that persists, by name: its absolute path, the same in both forms (the machine's
    /// volumes).
    #[serde(default)]
    pub data: IndexMap<DataName, AbsPath>,
    /// When the service counts as healthy beyond its ports answering (the spec's
    /// `healthcheck`).
    #[serde(default)]
    pub health: Option<Healthcheck>,
    /// The daemons the entry runs, by the key a `service` step names: added to the daemon table.
    #[serde(default)]
    pub daemons: IndexMap<UnitName, Daemon>,
    /// How the settings reach the official image, when the entry has one.
    #[serde(default)]
    pub image: Option<ImageUse>,
    /// The recipe: `install`, `configure` and `start`, each a list of steps.
    pub recipe: Recipe,
}

/// The licence of what an entry installs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Licence {
    /// The software's licence, as an SPDX expression (`BSD-2-Clause`).
    pub software: String,
    /// What else to know (the images bundle their base distribution's packages).
    #[serde(default)]
    pub notes: Option<String>,
}

/// One of an entry's ports: its number, and what it speaks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(try_from = "EntryPortInput")]
#[schemars(with = "EntryPortInput")]
pub struct EntryPort {
    /// Where its number comes from.
    pub number: PortNumber,
    /// What it speaks (default `tcp`).
    pub protocol: PortProtocol,
}

/// Where an entry's port number comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortNumber {
    /// A fixed number.
    Fixed(NonZeroU16),
    /// A `port` setting's value.
    Setting(SettingName),
}

/// A port as written: exactly one of `port` and `setting`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryPortInput {
    /// Its number, when it is fixed.
    #[serde(default)]
    port: Option<NonZeroU16>,
    /// The `port` setting giving its number.
    #[serde(default)]
    setting: Option<SettingName>,
    /// What it speaks: `tcp` (default), `udp`, `http`, `https` or `tls`.
    #[serde(default)]
    protocol: PortProtocol,
}

impl TryFrom<EntryPortInput> for EntryPort {
    type Error = String;

    fn try_from(p: EntryPortInput) -> Result<EntryPort, String> {
        let number = match (p.port, p.setting) {
            (Some(n), None) => PortNumber::Fixed(n),
            (None, Some(s)) => PortNumber::Setting(s),
            _ => return Err("a port takes exactly one of `port` and `setting`".to_owned()),
        };
        Ok(EntryPort { number, protocol: p.protocol })
    }
}

/// How the settings reach the official image (design §4.5): its own entrypoint reads variables
/// and files, or Isoloom's runner runs the recipe's `start` steps first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageUse {
    /// Environment variables the image reads, each a text with placeholders
    /// (`POSTGRES_PASSWORD_FILE: "{{ with.password.path }}"`).
    #[serde(default)]
    pub environment: IndexMap<String, String>,
    /// Paths in the image filled from a `folder` or `file` setting
    /// (`/docker-entrypoint-initdb.d: "{{ with.init }}"`).
    #[serde(default)]
    pub files: IndexMap<AbsPath, String>,
    /// The image's command, in place of its own (its arguments), with placeholders.
    #[serde(default)]
    pub command: Option<Vec<String>>,
    /// Who runs the `start` steps: the image's own entrypoint (`image`, the default), or
    /// Isoloom's runner before it (`run`).
    #[serde(default)]
    pub start: ImageStart,
}

/// Who prepares the official image at each start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ImageStart {
    /// The image's own entrypoint, fed by `environment` and `files`.
    #[default]
    Image,
    /// Isoloom's runner runs the recipe's `start` steps, then `exec`s the image's own
    /// entrypoint and command (recorded in `lock.yml`).
    Run,
}

impl Definition {
    /// The release `version` names exactly.
    #[must_use]
    pub fn release(&self, version: &Version) -> Option<&Release> {
        self.releases.iter().find(|r| &r.version == version)
    }

    /// The releases, as messages list them (`1.30.6 (stable), 1.31.7 (mainline)`).
    #[must_use]
    pub fn releases_list(&self) -> String {
        self.releases.iter().map(Release::label).collect::<Vec<_>>().join(", ")
    }
}
