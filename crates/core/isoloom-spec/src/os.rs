//! A machine's OS, written `<family>@<version>`:
//!
//! ```yaml
//! machines:
//!   web: { os: debian@12, ... }        # Debian 12, as a VM or its container image
//!   dc01:
//!     os: windows-server               # the family alone: its pinned default, 2022
//!     app: { steps: [{ run: dc.ps1 }] }
//! ```
//!
//! Those are the only spellings. Once the spec is lowered, each `vm.os` holds the image table's
//! name for its OS (`debian@12` is `debian-12`, [`Os::name`]), the name every target reads. A
//! version left out resolves to the family's pinned default ([`Family::default`]), recorded in
//! the resolved snapshot.

use crate::{Error, Result};

/// An OS family: the versions Isoloom has images for, each with its image-table name, and the
/// version an omitted one resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    /// The family's name, the part before `@` (`debian`, `windows-server`).
    pub name: &'static str,
    /// Its versions, oldest first, each with the image-table name it maps to.
    pub versions: &'static [(&'static str, &'static str)],
    /// The version `os: <family>` resolves to: pinned, so an environment never changes under
    /// its author when a newer version is added.
    pub default: &'static str,
}

/// The OS families, with their versions and pinned defaults.
pub const FAMILIES: &[Family] = &[
    Family {
        name: "debian",
        versions: &[("11", "debian-11"), ("12", "debian-12"), ("13", "debian-13")],
        default: "12",
    },
    Family {
        name: "ubuntu",
        versions: &[
            ("14.04", "ubuntu-14.04"),
            ("16.04", "ubuntu-16.04"),
            ("18.04", "ubuntu-18.04"),
            ("20.04", "ubuntu-20.04"),
            ("22.04", "ubuntu-22.04"),
            ("24.04", "ubuntu-24.04"),
        ],
        default: "24.04",
    },
    Family {
        name: "rocky",
        versions: &[("9", "rocky-9")],
        default: "9",
    },
    Family {
        name: "alma",
        versions: &[("9", "almalinux-9")],
        default: "9",
    },
    Family {
        name: "alpine",
        versions: &[("3.19", "alpine-3.19"), ("3.24", "alpine-3.24")],
        default: "3.19",
    },
    Family {
        name: "kali",
        versions: &[("rolling", "kali")],
        default: "rolling",
    },
    Family {
        name: "fedora",
        versions: &[("42", "fedora-42")],
        default: "42",
    },
    Family {
        name: "centos",
        versions: &[("7", "centos-7")],
        default: "7",
    },
    Family {
        name: "windows",
        versions: &[("10", "windows-10"), ("11", "windows-11")],
        default: "11",
    },
    Family {
        name: "windows-server",
        versions: &[
            ("2008r2", "windows-server-2008r2"),
            ("2012r2", "windows-server-2012r2"),
            ("2016", "windows-server-2016"),
            ("2019", "windows-server-2019"),
            ("2022", "windows-server-2022"),
            ("2025", "windows-server-2025"),
        ],
        default: "2022",
    },
];

/// An OS resolved: its family, its version (the default filled in) and its image-table name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Os {
    /// The family (`debian`).
    pub family: &'static str,
    /// The version (`12`).
    pub version: &'static str,
    /// The image table's name for it (`debian-12`), what the targets read.
    pub name: &'static str,
}

impl Os {
    /// How the spec writes it in full: `debian@12`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}@{}", self.family, self.version)
    }
}

impl Family {
    /// The family's version `version`, resolved.
    fn get(&self, version: &str) -> Option<Os> {
        self.versions.iter().find(|(v, _)| *v == version).map(|(v, n)| Os {
            family: self.name,
            version: v,
            name: n,
        })
    }

    /// Its versions, as an error lists them (`11, 12, 13`).
    fn known(&self) -> String {
        self.versions.iter().map(|(v, _)| *v).collect::<Vec<_>>().join(", ")
    }
}

/// Resolves an OS as a spec writes it: `<family>@<version>`, or `<family>` for its default
/// version.
///
/// # Errors
///
/// [`Error::UnknownOs`] listing the known families, or [`Error::UnknownOsVersion`] listing the
/// family's versions.
pub fn resolve(text: &str) -> Result<Os> {
    let (family, version) = text.split_once('@').map_or((text, None), |(f, v)| (f, Some(v)));
    let Some(f) = FAMILIES.iter().find(|f| f.name == family) else {
        return Err(Error::UnknownOs {
            text: text.to_owned(),
            known: FAMILIES.iter().map(|f| format!("{} ({})", f.name, f.known())).collect::<Vec<_>>().join(", "),
        });
    };
    let version = version.unwrap_or(f.default);
    f.get(version).ok_or_else(|| Error::UnknownOsVersion {
        family: f.name,
        version: version.to_owned(),
        known: f.known(),
        default: f.default,
    })
}

/// The OS an image-table name (`debian-12`) stands for: how a parsed spec's `vm.os` names it.
#[must_use]
pub fn by_name(name: &str) -> Option<Os> {
    FAMILIES
        .iter()
        .find_map(|f| f.versions.iter().find(|(_, n)| *n == name).and_then(|(v, _)| f.get(v)))
}

/// Every spelling an `os` field takes: each family alone and each `<family>@<version>`. The
/// schema offers them for completion.
#[must_use]
pub fn spellings() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in FAMILIES {
        out.push(f.name.to_owned());
        out.extend(f.versions.iter().map(|(v, _)| format!("{}@{v}", f.name)));
    }
    out
}

mod containers;

pub use containers::container_image;

#[cfg(test)]
mod tests;
