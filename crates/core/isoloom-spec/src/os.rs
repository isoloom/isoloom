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
//! version left out resolves to the distribution's pinned default ([`Distro::default`]),
//! recorded in the resolved snapshot.

use crate::{Error, Result};

/// How a distribution installs its packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Installer {
    /// apt (Debian, Ubuntu, Kali).
    Apt,
    /// dnf, yum on CentOS 7 (Rocky, AlmaLinux, Fedora, CentOS).
    Dnf,
    /// apk (Alpine).
    Apk,
    /// Windows features (Windows, Windows Server).
    Windows,
}

/// An OS distribution, the `<family>` of `os: <family>@<version>`: the versions Isoloom has
/// images for, each with its image-table name, the version an omitted one resolves to, and how
/// it installs packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Distro {
    /// Its name, the part before `@` (`debian`, `windows-server`).
    pub name: &'static str,
    /// Its versions, oldest first, each with the image-table name it maps to.
    pub versions: &'static [(&'static str, &'static str)],
    /// The version `os: <family>` resolves to: pinned, so an environment never changes under
    /// its author when a newer version is added.
    pub default: &'static str,
    /// How it installs packages.
    pub installer: Installer,
}

/// The OS distributions, with their versions and pinned defaults.
pub const DISTROS: &[Distro] = &[
    Distro {
        name: "debian",
        versions: &[("11", "debian-11"), ("12", "debian-12"), ("13", "debian-13")],
        default: "12",
        installer: Installer::Apt,
    },
    Distro {
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
        installer: Installer::Apt,
    },
    Distro {
        name: "rocky",
        versions: &[("9", "rocky-9")],
        default: "9",
        installer: Installer::Dnf,
    },
    Distro {
        name: "alma",
        versions: &[("9", "almalinux-9")],
        default: "9",
        installer: Installer::Dnf,
    },
    Distro {
        name: "alpine",
        versions: &[("3.19", "alpine-3.19"), ("3.24", "alpine-3.24")],
        default: "3.19",
        installer: Installer::Apk,
    },
    Distro {
        name: "kali",
        versions: &[("rolling", "kali")],
        default: "rolling",
        installer: Installer::Apt,
    },
    Distro {
        name: "fedora",
        versions: &[("42", "fedora-42")],
        default: "42",
        installer: Installer::Dnf,
    },
    Distro {
        name: "centos",
        versions: &[("7", "centos-7")],
        default: "7",
        installer: Installer::Dnf,
    },
    Distro {
        name: "windows",
        versions: &[("10", "windows-10"), ("11", "windows-11")],
        default: "11",
        installer: Installer::Windows,
    },
    Distro {
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
        installer: Installer::Windows,
    },
];

/// An OS resolved: its distribution, its version (the default filled in), its image-table name
/// and how it installs packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Os {
    /// The distribution's name, the `<family>` of `<family>@<version>` (`debian`).
    pub family: &'static str,
    /// The version (`12`).
    pub version: &'static str,
    /// The image table's name for it (`debian-12`), what the targets read.
    pub name: &'static str,
    /// How it installs packages.
    pub installer: Installer,
}

/// What kind of OS: the one rule that tells Windows (WinRM, PowerShell, no container form) from
/// Linux (SSH, sh), in one place for every target and tool (Clean Code J3, G5: 76 string checks
/// told it apart by a prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Reached over SSH, provisioned by sh; a container form exists.
    Linux,
    /// Reached over WinRM, provisioned by PowerShell; VMs only.
    Windows,
}

impl Kind {
    /// The kind of the OS an image-table name (`windows-11`, `debian-12`) or a family names.
    #[must_use]
    pub fn of(name: &str) -> Kind {
        if name.starts_with("windows") { Kind::Windows } else { Kind::Linux }
    }

    /// Whether it is Windows.
    #[must_use]
    pub fn is_windows(self) -> bool {
        self == Kind::Windows
    }
}

impl Os {
    /// Its kind: Windows or Linux.
    #[must_use]
    pub fn kind(&self) -> Kind {
        Kind::of(self.name)
    }

    /// How the spec writes it in full: `debian@12`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}@{}", self.family, self.version)
    }

    /// Whether it installs with apt: the one rule for an apt repository's `suite` and
    /// `components`.
    #[must_use]
    pub fn installs_with_apt(&self) -> bool {
        self.installer == Installer::Apt
    }
}

impl Distro {
    /// The distribution's version `version`, resolved.
    fn get(&self, version: &str) -> Option<Os> {
        self.versions.iter().find(|(v, _)| *v == version).map(|(v, n)| Os {
            family: self.name,
            version: v,
            name: n,
            installer: self.installer,
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
    let Some(f) = DISTROS.iter().find(|f| f.name == family) else {
        return Err(Error::UnknownOs {
            text: text.to_owned(),
            known: DISTROS.iter().map(|f| format!("{} ({})", f.name, f.known())).collect::<Vec<_>>().join(", "),
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
    DISTROS
        .iter()
        .find_map(|f| f.versions.iter().find(|(_, n)| *n == name).and_then(|(v, _)| f.get(v)))
}

/// Every spelling an `os` field takes: each family alone and each `<family>@<version>`. The
/// schema offers them for completion.
#[must_use]
pub fn spellings() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in DISTROS {
        out.push(f.name.to_owned());
        out.extend(f.versions.iter().map(|(v, _)| format!("{}@{v}", f.name)));
    }
    out
}

mod containers;

pub use containers::container_image;

#[cfg(test)]
mod tests;
