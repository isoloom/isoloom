//! Where a machine keeps data: how it mounts a volume (all of it, or one folder of a shared one),
//! its memory-backed mounts, and the data disks a VM is given beside its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where a machine mounts one of its volumes: a path, or a path and the folder of a shared
/// volume it takes (`{ path: /srv/www, subpath: www }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Mount {
    /// The whole volume at this absolute path.
    Path(String),
    /// A mount with its options.
    Part(MountPart),
}

/// A mount with its options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MountPart {
    /// The absolute path in the machine.
    pub path: String,
    /// One folder of a shared volume (`www`, `logs/app`), relative to the volume's root, instead
    /// of all of it. Created when missing. The volume's owner holds all of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
}

impl Mount {
    /// The absolute path in the machine.
    pub fn path(&self) -> &str {
        match self {
            Mount::Path(p) => p,
            Mount::Part(m) => &m.path,
        }
    }

    /// The folder of a shared volume it takes, if not all of it.
    pub fn subpath(&self) -> Option<&str> {
        match self {
            Mount::Path(_) => None,
            Mount::Part(m) => m.subpath.as_deref(),
        }
    }
}

impl From<&str> for Mount {
    fn from(path: &str) -> Mount {
        Mount::Path(path.to_string())
    }
}

// A string is a path; a map is a mount with options. Written by hand (not `untagged`) so a typo
// in the map is reported with its field instead of "no variant matched".
impl<'de> Deserialize<'de> for Mount {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Mount;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an absolute path, or `{ path, subpath }`")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Mount, E> {
                Ok(Mount::Path(v.to_string()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Mount, A::Error> {
                MountPart::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Mount::Part)
            }
        }
        d.deserialize_any(V)
    }
}

/// A memory-backed mount: a path, or a path with a size and a mode
/// (`{ path: /run, size: 64m, mode: "1777" }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Tmpfs {
    /// A mount at this absolute path, of the runtime's default size and mode.
    Path(String),
    /// A mount with its options.
    Sized(TmpfsMount),
}

/// A memory-backed mount with its options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TmpfsMount {
    /// The absolute path in the machine.
    pub path: String,
    /// Its most memory: bytes, or a number with `k`, `m` or `g` (`64m`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Its permissions, in octal (`"1777"`, `"0700"`). Quote it: YAML reads `0700` unquoted as
    /// a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Tmpfs {
    /// The absolute path in the machine.
    pub fn path(&self) -> &str {
        match self {
            Tmpfs::Path(p) => p,
            Tmpfs::Sized(t) => &t.path,
        }
    }

    /// Its size, when given.
    pub fn size(&self) -> Option<&str> {
        match self {
            Tmpfs::Path(_) => None,
            Tmpfs::Sized(t) => t.size.as_deref(),
        }
    }

    /// Its mode, when given (octal digits).
    pub fn mode(&self) -> Option<&str> {
        match self {
            Tmpfs::Path(_) => None,
            Tmpfs::Sized(t) => t.mode.as_deref(),
        }
    }
}

impl From<&str> for Tmpfs {
    fn from(path: &str) -> Tmpfs {
        Tmpfs::Path(path.to_string())
    }
}

impl<'de> Deserialize<'de> for Tmpfs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Tmpfs;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an absolute path, or `{ path, size, mode }`")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Tmpfs, E> {
                Ok(Tmpfs::Path(v.to_string()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Tmpfs, A::Error> {
                TmpfsMount::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Tmpfs::Sized)
            }
        }
        d.deserialize_any(V)
    }
}

/// A data disk a machine is given beside its own: on a VM, a disk of its own, formatted and
/// mounted at boot (a raw one is left as it is); on the container targets, a volume at the
/// mount (a claim of that size on Kubernetes). It goes with the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Disk {
    /// Its size in GB (1 to 16384).
    pub size_gb: u32,
    /// Its filesystem: `ext4` (the default on Linux), `xfs`, `ntfs` (the default on Windows), or
    /// `raw` for a block device left unformatted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs: Option<Filesystem>,
    /// Where it is mounted: an absolute path on Linux and in containers, a drive letter (`E:`)
    /// on Windows. Required unless the disk is `raw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,
}

/// A data disk's filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Filesystem {
    /// Linux's usual filesystem.
    Ext4,
    /// XFS, for large files.
    Xfs,
    /// Windows' filesystem.
    Ntfs,
    /// No filesystem: a block device the machine formats or uses itself.
    Raw,
}

impl Filesystem {
    /// The name `mkfs` and fstab know it by.
    pub fn id(self) -> &'static str {
        match self {
            Filesystem::Ext4 => "ext4",
            Filesystem::Xfs => "xfs",
            Filesystem::Ntfs => "ntfs",
            Filesystem::Raw => "raw",
        }
    }
}
