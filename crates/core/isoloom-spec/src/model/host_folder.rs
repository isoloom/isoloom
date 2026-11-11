//! A folder of the computer that runs the environment, mounted in a machine (`host_folders`): the same
//! folder on both sides, so what the machine writes there lands on that computer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A folder of the computer that runs the environment, mounted in the machine both ways: what the machine
/// writes there lands on that computer, and what is put there shows in the machine. Only the
/// targets whose machines run on that computer mount one (`docker`, `docker-vm` on a local
/// provider, `vagrant`); the others refuse a spec that asks for one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostFolder {
    /// The folder on the computer that runs the environment: relative to the project folder (`shared`,
    /// `../notes`), in the home folder of the user running it (`~/work`), or absolute
    /// (`/srv/share`, `C:\share`). A project folder must exist; a home or absolute one is the
    /// running computer's (the environment may run on another computer than the one generating it), so
    /// it isn't checked, and it is created when missing.
    pub from: String,
    /// The absolute path in the machine (Linux machines), not inside Isoloom's own folders
    /// (`/opt/isoloom`, `/var/lib/isoloom`, `/etc/isoloom`) or the kernel's (`/proc`, `/sys`,
    /// `/dev`), nor one of its volumes, disks or tmpfs mounts.
    pub path: String,
    /// The machine only reads it (it can't write there). Off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

/// Where a host folder is on the computer that runs the environment, as `from` writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPath<'a> {
    /// Relative to the project folder.
    Project(&'a str),
    /// In the home folder of the user running the environment: `~` alone, or `~/<rest>`.
    Home(&'a str),
    /// An absolute path (`/srv/share`, `C:\share`).
    Absolute(&'a str),
}

impl HostFolder {
    /// Where `from` is.
    #[must_use]
    pub fn host(&self) -> HostPath<'_> {
        let from = self.from.as_str();
        if from == "~" || from.starts_with("~/") || from.starts_with("~\\") {
            HostPath::Home(from)
        } else if from.starts_with(['/', '\\']) || super::is_drive_path(from) {
            HostPath::Absolute(from)
        } else {
            HostPath::Project(from)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(from: &str) -> HostPath<'_> {
        match from {
            "~" | "~/notes" => HostPath::Home(from),
            "/srv/share" | "C:\\share" | "D:/data" => HostPath::Absolute(from),
            _ => HostPath::Project(from),
        }
    }

    #[test]
    fn from_is_a_project_folder_a_home_folder_or_an_absolute_one() -> Result<(), serde_yaml_ng::Error> {
        for from in ["shared", "../notes", "~", "~/notes", "/srv/share", "C:\\share", "D:/data", "~other"] {
            let f: HostFolder = serde_yaml_ng::from_str(&format!("{{ from: '{from}', path: /mnt/x }}"))?;
            assert_eq!(f.host(), host(from), "{from}");
            assert!(!f.read_only);
        }
        let ro: HostFolder = serde_yaml_ng::from_str("{ from: x, path: /x, read_only: true }")?;
        assert!(ro.read_only);
        assert!(serde_yaml_ng::from_str::<HostFolder>("{ from: x, path: /x, ro: true }").is_err());
        Ok(())
    }
}
