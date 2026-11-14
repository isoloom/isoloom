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

/// What kind of OS: the one rule that tells Windows (WinRM, PowerShell, no container form) from
/// Linux (SSH, sh). Each distribution of [`DISTROS`] says which it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Reached over SSH, provisioned by sh; a container form exists.
    Linux,
    /// Reached over WinRM, provisioned by PowerShell; VMs only.
    Windows,
}

impl Kind {
    /// The kind of the OS an image-table name (`windows-11`, `debian-12`), a family
    /// (`windows-server`) or a spec's `<family>@<version>` (`windows-server@2019`) names, as
    /// [`DISTROS`] says; Linux for a name the table doesn't have.
    #[must_use]
    pub fn of(name: &str) -> Kind {
        let family = name.split_once('@').map_or(name, |(family, _)| family);
        by_name(name)
            .map(|os| os.kind)
            .or_else(|| DISTROS.iter().find(|d| d.name == family).map(|d| d.kind))
            .unwrap_or(Kind::Linux)
    }

    /// Whether it is Windows.
    #[must_use]
    pub fn is_windows(self) -> bool {
        self == Kind::Windows
    }
}

/// One version of a distribution, and every fact Isoloom keeps about it: one row, so adding a
/// version is one place and no side table can miss it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Release {
    /// The version, the part after `@` (`12`).
    pub version: &'static str,
    /// The image table's name for it (`debian-12`), what the targets read.
    pub name: &'static str,
    /// Its apt codename (`bookworm`): an apt repository's default suite. `None` off apt.
    pub codename: Option<&'static str>,
    /// Its official container image, pinned by digest: the container form of a machine that is
    /// its OS alone, and the base of the Dockerfile generated from a machine's portable steps. A
    /// digest is the image's multi-architecture index, so `amd64` and `arm64` machines pull the
    /// same reference. `None` for Windows. Refreshed by hand:
    /// `docker buildx imagetools inspect <image> --format '{{json .Manifest.Digest}}'`.
    pub container: Option<&'static str>,
    /// Whether it expects a TPM 2.0: Windows 11 refuses to install or upgrade without one.
    pub expects_tpm: bool,
}

/// An OS distribution, the `<family>` of `os: <family>@<version>`: its kind, how it installs
/// packages, the version an omitted one resolves to, and its releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Distro {
    /// Its name, the part before `@` (`debian`, `windows-server`).
    pub name: &'static str,
    /// Windows or Linux.
    pub kind: Kind,
    /// How it installs packages.
    pub installer: Installer,
    /// The version `os: <family>` resolves to: pinned, so an environment never changes under
    /// its author when a newer version is added.
    pub default: &'static str,
    /// Its releases, oldest first.
    pub releases: &'static [Release],
}

/// The OS table: every distribution and release Isoloom has images for, with everything known of
/// each.
pub const DISTROS: &[Distro] = &[
    Distro {
        name: "debian",
        kind: Kind::Linux,
        installer: Installer::Apt,
        default: "12",
        releases: &[
            Release {
                version: "11",
                name: "debian-11",
                codename: Some("bullseye"),
                container: Some("debian:11@sha256:6f519a81440354a85eb592c5f32109ab80605f6b892455983a6f618bf87fabe9"),
                expects_tpm: false,
            },
            Release {
                version: "12",
                name: "debian-12",
                codename: Some("bookworm"),
                container: Some("debian:12@sha256:2c037a04925515fdd6ea85ea14a682d0e79931f5e9f5d07b6dbfc6ba12f9e858"),
                expects_tpm: false,
            },
            Release {
                version: "13",
                name: "debian-13",
                codename: Some("trixie"),
                container: Some("debian:13@sha256:913f6706df59a68922d1dd08f78c2476560a8d367897200a6005b00e5f67c2d5"),
                expects_tpm: false,
            },
        ],
    },
    Distro {
        name: "ubuntu",
        kind: Kind::Linux,
        installer: Installer::Apt,
        default: "24.04",
        releases: &[
            Release {
                version: "14.04",
                name: "ubuntu-14.04",
                codename: Some("trusty"),
                container: Some("ubuntu:14.04@sha256:64483f3496c1373bfd55348e88694d1c4d0c9b660dee6bfef5e12f43b9933b30"),
                expects_tpm: false,
            },
            Release {
                version: "16.04",
                name: "ubuntu-16.04",
                codename: Some("xenial"),
                container: Some("ubuntu:16.04@sha256:1f1a2d56de1d604801a9671f301190704c25d604a416f59e03c04f5c6ffee0d6"),
                expects_tpm: false,
            },
            Release {
                version: "18.04",
                name: "ubuntu-18.04",
                codename: Some("bionic"),
                container: Some("ubuntu:18.04@sha256:152dc042452c496007f07ca9127571cb9c29697f42acbfad72324b2bb2e43c98"),
                expects_tpm: false,
            },
            Release {
                version: "20.04",
                name: "ubuntu-20.04",
                codename: Some("focal"),
                container: Some("ubuntu:20.04@sha256:8feb4d8ca5354def3d8fce243717141ce31e2c428701f6682bd2fafe15388214"),
                expects_tpm: false,
            },
            Release {
                version: "22.04",
                name: "ubuntu-22.04",
                codename: Some("jammy"),
                container: Some("ubuntu:22.04@sha256:5ec03bb3441e8b0bf3b4f9cd4629a1ae763010dc3035bb8da3ae6cf026486401"),
                expects_tpm: false,
            },
            Release {
                version: "24.04",
                name: "ubuntu-24.04",
                codename: Some("noble"),
                container: Some("ubuntu:24.04@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55"),
                expects_tpm: false,
            },
        ],
    },
    Distro {
        name: "rocky",
        kind: Kind::Linux,
        installer: Installer::Dnf,
        default: "9",
        releases: &[Release {
            version: "9",
            name: "rocky-9",
            codename: None,
            container: Some("rockylinux/rockylinux:9@sha256:8101994123cf3d0a8fee517bee7f39e555c7d92bd2d9eb3303cc988a0eeed00f"),
            expects_tpm: false,
        }],
    },
    Distro {
        name: "alma",
        kind: Kind::Linux,
        installer: Installer::Dnf,
        default: "9",
        releases: &[Release {
            version: "9",
            name: "almalinux-9",
            codename: None,
            container: Some("almalinux:9@sha256:9819dc675b67b595c2b59e42be7763fca1a8bb217fa5944e04daa22e9a64db16"),
            expects_tpm: false,
        }],
    },
    Distro {
        name: "alpine",
        kind: Kind::Linux,
        installer: Installer::Apk,
        default: "3.19",
        releases: &[
            Release {
                version: "3.19",
                name: "alpine-3.19",
                codename: None,
                container: Some("alpine:3.19@sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1"),
                expects_tpm: false,
            },
            Release {
                version: "3.24",
                name: "alpine-3.24",
                codename: None,
                container: Some("alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6"),
                expects_tpm: false,
            },
        ],
    },
    Distro {
        name: "kali",
        kind: Kind::Linux,
        installer: Installer::Apt,
        default: "rolling",
        releases: &[Release {
            version: "rolling",
            name: "kali",
            codename: Some("kali-rolling"),
            container: Some("kalilinux/kali-rolling@sha256:02f5d6c019e549b0f55da58e835cad5355ef1030c6b2645265a324e37dfc4acc"),
            expects_tpm: false,
        }],
    },
    Distro {
        name: "fedora",
        kind: Kind::Linux,
        installer: Installer::Dnf,
        default: "42",
        releases: &[Release {
            version: "42",
            name: "fedora-42",
            codename: None,
            container: Some("fedora:42@sha256:99e203b80b1c3d8f7e161ec10a68fd02b081ef83a3963553e513c82846b97814"),
            expects_tpm: false,
        }],
    },
    Distro {
        name: "centos",
        kind: Kind::Linux,
        installer: Installer::Dnf,
        default: "7",
        releases: &[Release {
            version: "7",
            name: "centos-7",
            codename: None,
            container: Some("centos:7@sha256:be65f488b7764ad3638f236b7b515b3678369a5124c47b8d32916d6487418ea4"),
            expects_tpm: false,
        }],
    },
    Distro {
        name: "windows",
        kind: Kind::Windows,
        installer: Installer::Windows,
        default: "11",
        releases: &[
            Release {
                version: "10",
                name: "windows-10",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "11",
                name: "windows-11",
                codename: None,
                container: None,
                expects_tpm: true,
            },
        ],
    },
    Distro {
        name: "windows-server",
        kind: Kind::Windows,
        installer: Installer::Windows,
        default: "2022",
        releases: &[
            Release {
                version: "2008r2",
                name: "windows-server-2008r2",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "2012r2",
                name: "windows-server-2012r2",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "2016",
                name: "windows-server-2016",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "2019",
                name: "windows-server-2019",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "2022",
                name: "windows-server-2022",
                codename: None,
                container: None,
                expects_tpm: false,
            },
            Release {
                version: "2025",
                name: "windows-server-2025",
                codename: None,
                container: None,
                expects_tpm: false,
            },
        ],
    },
];

/// An OS resolved: its distribution's facts and its release's, the default version filled in.
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
    /// Windows or Linux.
    pub kind: Kind,
    /// Its row of the OS table.
    pub release: &'static Release,
}

impl Os {
    /// Its kind: Windows or Linux.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
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

    /// Its apt codename (`bookworm`), `None` off apt.
    #[must_use]
    pub fn codename(&self) -> Option<&'static str> {
        self.release.codename
    }

    /// Its official container image, pinned by digest; `None` for Windows.
    #[must_use]
    pub fn container_image(&self) -> Option<&'static str> {
        self.release.container
    }

    /// Whether it expects a TPM 2.0 (Windows 11).
    #[must_use]
    pub fn expects_tpm(&self) -> bool {
        self.release.expects_tpm
    }
}

impl Distro {
    /// The distribution's version `version`, resolved.
    fn get(&'static self, version: &str) -> Option<Os> {
        self.releases.iter().find(|r| r.version == version).map(|r| self.os(r))
    }

    /// `release`, one of its own, resolved.
    fn os(&'static self, release: &'static Release) -> Os {
        Os {
            family: self.name,
            version: release.version,
            name: release.name,
            installer: self.installer,
            kind: self.kind,
            release,
        }
    }

    /// Its versions, as an error lists them (`11, 12, 13`).
    fn known(&self) -> String {
        self.releases.iter().map(|r| r.version).collect::<Vec<_>>().join(", ")
    }
}

/// Every OS of the table, in its order: each distribution's releases, oldest first.
pub fn all() -> impl Iterator<Item = Os> {
    DISTROS.iter().flat_map(|d| d.releases.iter().map(move |r| d.os(r)))
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
    all().find(|os| os.name == name)
}

/// Every spelling an `os` field takes: each family alone and each `<family>@<version>`. The
/// schema offers them for completion.
#[must_use]
pub fn spellings() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in DISTROS {
        out.push(f.name.to_owned());
        out.extend(f.releases.iter().map(|r| format!("{}@{}", f.name, r.version)));
    }
    out
}

#[cfg(test)]
mod tests;
