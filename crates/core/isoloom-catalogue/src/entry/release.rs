//! An entry's releases (design §4.3): each a version, its upstream channel, where the VM form
//! installs it from, the OSes it runs on, and the variables its recipe reads.

use indexmap::IndexMap;
use isoloom_spec::syntax::OsRef;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::names::VarName;
use crate::version::Version;

/// One release of the service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// The upstream version: exact for `vendor` and `download` (`1.30.6`), the series for
    /// `distro` (`2.4`).
    pub version: Version,
    /// Its upstream channel (`stable`, `mainline`, `lts`), when upstream has several.
    #[serde(default)]
    pub channel: Option<String>,
    /// Where the VM form installs it from.
    pub source: Source,
    /// The OSes it runs on, each in full (`debian@12`): its first is the one the parity
    /// contract's exit run uses.
    pub os: Vec<OsRef>,
    /// Variables its recipe reads as `{{ vars.<name> }}`.
    #[serde(default)]
    pub vars: IndexMap<VarName, String>,
}

/// Where a release comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Source {
    /// The vendor's own package repository, pinned to an exact version (`lock.yml` holds each
    /// OS's package version string).
    Vendor,
    /// The OS's own packages: pinned to a series, since the archive moves within it.
    Distro,
    /// A pinned archive (a `file` step with `url` and `sha256`).
    Download,
}

impl Source {
    /// How an entry writes it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Source::Vendor => "vendor",
            Source::Distro => "distro",
            Source::Download => "download",
        }
    }
}

impl Release {
    /// The release as messages list it (`1.30.6 (stable)`).
    #[must_use]
    pub fn label(&self) -> String {
        match &self.channel {
            Some(c) => format!("{} ({c})", self.version),
            None => self.version.to_string(),
        }
    }

    /// Whether it runs on `os`.
    #[must_use]
    pub fn runs_on(&self, os: OsRef) -> bool {
        self.os.contains(&os)
    }

    /// Its OSes, as messages list them (`debian@12, alpine@3.24`).
    #[must_use]
    pub fn os_list(&self) -> String {
        self.os.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    }

    /// Whether `when: { release: [...] }`'s `word` names it: its channel, or its version or a
    /// prefix of it.
    #[must_use]
    pub fn is(&self, word: &str) -> bool {
        self.channel.as_deref() == Some(word) || word.parse::<Version>().is_ok_and(|v| self.version.starts_with(&v))
    }
}
