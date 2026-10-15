//! A machine's OS, written `<family>@<version>`:
//!
//! ```yaml
//! machines:
//!   web: { os: debian@12, ... }        # a VM of Debian 12
//!   dc01:
//!     os: windows-server               # the family alone: its pinned default, 2022
//!     vm: { provision: [dc.ps1] }      # `vm.os` defaults to the machine's `os`
//! ```
//!
//! Those are the only spellings. Once the spec is parsed, each `vm.os` holds the image table's
//! name for its OS (`debian@12` is `debian-12`), the name every target reads ([`canonical`]). A
//! version left out resolves to the family's pinned default ([`Family::default`]), recorded in
//! the resolved snapshot. The machine's `os` reaches its `vm:` on the YAML, before `common:` and
//! the groups fold in ([`fill_vm`]), so a machine's own OS wins over a shared one, as its own
//! `vm.os` does.

use serde_yaml_ng::{Mapping, Value};

use crate::model::Spec;

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
        versions: &[("3.19", "alpine-3.19")],
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
    pub fn id(&self) -> String {
        format!("{}@{}", self.family, self.version)
    }
}

/// Why an OS doesn't resolve. Each message lists what is known.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OsError {
    /// Neither a family nor an image-table name.
    #[error("unknown OS `{text}`; write `<family>@<version>` with one of: {known}")]
    UnknownFamily {
        /// What the spec wrote.
        text: String,
        /// The families and their versions.
        known: String,
    },
    /// A known family without that version.
    #[error("{family} has no version `{version}`; use one of: {known} (`{family}` alone is {family}@{default})")]
    UnknownVersion {
        /// The family.
        family: &'static str,
        /// The version the spec wrote.
        version: String,
        /// The family's versions.
        known: String,
        /// The family's default version.
        default: &'static str,
    },
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
pub fn resolve(text: &str) -> Result<Os, OsError> {
    let (family, version) = text.split_once('@').map_or((text, None), |(f, v)| (f, Some(v)));
    let Some(f) = FAMILIES.iter().find(|f| f.name == family) else {
        return Err(OsError::UnknownFamily {
            text: text.to_string(),
            known: FAMILIES.iter().map(|f| format!("{} ({})", f.name, f.known())).collect::<Vec<_>>().join(", "),
        });
    };
    let version = version.unwrap_or(f.default);
    f.get(version).ok_or_else(|| OsError::UnknownVersion {
        family: f.name,
        version: version.to_string(),
        known: f.known(),
        default: f.default,
    })
}

/// The OS an image-table name (`debian-12`) stands for: how a parsed spec's `vm.os` names it.
pub fn by_name(name: &str) -> Option<Os> {
    FAMILIES
        .iter()
        .find_map(|f| f.versions.iter().find(|(_, n)| *n == name).and_then(|(v, _)| f.get(v)))
}

/// Every spelling an `os` field takes: each family alone and each `<family>@<version>`. The
/// schema offers them for completion.
pub fn spellings() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in FAMILIES {
        out.push(f.name.to_string());
        out.extend(f.versions.iter().map(|(v, _)| format!("{}@{v}", f.name)));
    }
    out
}

fn key(s: &str) -> Value {
    Value::String(s.to_string())
}

/// Gives each machine's `os` to its `vm:`, on the YAML before `common:` and the groups fold in:
/// a machine with neither `docker:` nor `vm:` gets `vm: { os }`, and a `vm:` without `os` gets
/// the machine's. The text is copied as written; [`canonical`] resolves both once parsed.
pub fn fill_vm(doc: &mut Value) {
    let Some(Value::Mapping(machines)) = doc.get_mut("machines") else { return };
    for m in machines.values_mut() {
        let Value::Mapping(m) = m else { continue };
        let Some(os) = m.get("os").filter(|v| v.is_string()).cloned() else { continue };
        if !m.contains_key("docker") && !m.contains_key("vm") {
            m.insert(key("vm"), Value::Mapping(Mapping::new()));
        }
        if let Some(Value::Mapping(vm)) = m.get_mut("vm")
            && !vm.contains_key("os")
        {
            vm.insert(key("os"), os);
        }
    }
}

/// Writes each machine's OS in its resolved forms: the machine's `os` as `<family>@<version>`
/// (the default version filled in), its `vm.os` as the image-table name every target reads.
/// An OS that doesn't resolve is refused, naming its field; an empty `vm.os` is left for
/// validation, which says how to name one.
pub fn canonical(spec: &mut Spec) -> Result<(), String> {
    for (name, m) in &mut spec.machines {
        if let Some(text) = &m.os {
            m.os = Some(resolve(text).map_err(|e| format!("machines.{name}.os: {e}"))?.id());
        }
        if let Some(vm) = m.vm.as_mut().filter(|vm| !vm.os.is_empty()) {
            vm.os = resolve(&vm.os).map_err(|e| format!("machines.{name}.vm.os: {e}"))?.name.to_string();
        }
    }
    Ok(())
}

mod containers;

pub use containers::container_image;

#[cfg(test)]
mod tests;
