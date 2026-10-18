//! How an OS installs and starts things: its family. The configuration renderers write one
//! implementation of each resource per family (apt and systemd, dnf and systemd, apk and OpenRC,
//! Windows); a recipe is resolved for the family of its machine's OS, and the daemon table names
//! a daemon's unit and package per family.

use isoloom_spec::os::Os;
use isoloom_spec::syntax::OsRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An OS family, as the configuration model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// Debian, Ubuntu and Kali: apt, systemd.
    Debian,
    /// Rocky, AlmaLinux, Fedora and CentOS: dnf (yum on CentOS 7), systemd.
    #[serde(rename = "redhat")]
    RedHat,
    /// Alpine: apk, OpenRC.
    Alpine,
    /// Windows and Windows Server: features, Windows services.
    Windows,
}

impl Family {
    /// Every family.
    pub const ALL: [Family; 4] = [Family::Debian, Family::RedHat, Family::Alpine, Family::Windows];

    /// The family of `os`.
    #[must_use]
    pub fn of(os: Os) -> Family {
        match os.family {
            "rocky" | "alma" | "fedora" | "centos" => Family::RedHat,
            "alpine" => Family::Alpine,
            "windows" | "windows-server" => Family::Windows,
            _ => Family::Debian,
        }
    }

    /// The family's name, as a refusal names it (`Debian`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Family::Debian => "Debian",
            Family::RedHat => "Red Hat",
            Family::Alpine => "Alpine",
            Family::Windows => "Windows",
        }
    }

    /// How an entry writes it (`redhat`).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Family::Debian => "debian",
            Family::RedHat => "redhat",
            Family::Alpine => "alpine",
            Family::Windows => "windows",
        }
    }
}

impl From<OsRef> for Family {
    fn from(os: OsRef) -> Family {
        Family::of(os.os())
    }
}

/// The codename of an apt release (`bookworm` for `debian@12`): an apt repository's default
/// suite, and `{{ os.codename }}`. `None` for an OS that has none.
#[must_use]
pub fn codename(os: Os) -> Option<&'static str> {
    let name = match (os.family, os.version) {
        ("debian", "11") => "bullseye",
        ("debian", "12") => "bookworm",
        ("debian", "13") => "trixie",
        ("ubuntu", "14.04") => "trusty",
        ("ubuntu", "16.04") => "xenial",
        ("ubuntu", "18.04") => "bionic",
        ("ubuntu", "20.04") => "focal",
        ("ubuntu", "22.04") => "jammy",
        ("ubuntu", "24.04") => "noble",
        ("kali", _) => "kali-rolling",
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(text: &str) -> Os {
        text.parse::<OsRef>().map_or_else(|e| panic!("{text}: {e}"), OsRef::os)
    }

    #[test]
    fn each_os_has_its_family() {
        for (text, family) in [
            ("debian@12", Family::Debian),
            ("ubuntu@24.04", Family::Debian),
            ("kali", Family::Debian),
            ("rocky@9", Family::RedHat),
            ("alma", Family::RedHat),
            ("centos@7", Family::RedHat),
            ("alpine", Family::Alpine),
            ("windows-server@2022", Family::Windows),
            ("windows@11", Family::Windows),
        ] {
            assert_eq!(Family::of(os(text)), family, "{text}");
        }
    }

    #[test]
    fn every_apt_release_has_its_codename() {
        for spelling in isoloom_spec::os::spellings() {
            let o = os(&spelling);
            assert_eq!(codename(o).is_some(), Family::of(o) == Family::Debian, "{spelling}");
        }
        assert_eq!(codename(os("debian@12")), Some("bookworm"));
    }

    #[test]
    fn an_entry_writes_each_family_by_its_id() {
        for f in Family::ALL {
            assert_eq!(serde_yaml_ng::from_str::<Family>(f.id()).ok(), Some(f));
        }
    }

    #[test]
    fn each_family_names_itself_as_a_refusal_reads_it() {
        assert_eq!(Family::Debian.name(), "Debian");
        assert_eq!(Family::RedHat.name(), "Red Hat");
        assert_eq!(Family::Alpine.name(), "Alpine");
        assert_eq!(Family::Windows.name(), "Windows");
    }
}
