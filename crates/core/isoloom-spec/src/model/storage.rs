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
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Mount::Path(p) => p,
            Mount::Part(m) => &m.path,
        }
    }

    /// The folder of a shared volume it takes, if not all of it.
    #[must_use]
    pub fn subpath(&self) -> Option<&str> {
        match self {
            Mount::Path(_) => None,
            Mount::Part(m) => m.subpath.as_deref(),
        }
    }
}

impl From<&str> for Mount {
    fn from(path: &str) -> Mount {
        Mount::Path(path.to_owned())
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
                Ok(Mount::Path(v.to_owned()))
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
    pub mode: Option<TmpfsMode>,
}

/// A tmpfs mount's permissions: three or four octal digits (`"1777"`: the sticky bit too), read
/// once, so every target writes the same bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TmpfsMode(u32);

impl TmpfsMode {
    /// The mode as a number (`0o1777`).
    #[must_use]
    pub fn bits(self) -> u32 {
        self.0
    }
}

impl std::str::FromStr for TmpfsMode {
    type Err = String;

    /// The mode `digits` names: three or four octal digits.
    ///
    /// # Errors
    ///
    /// Why they aren't one, as a spec's problem says it.
    fn from_str(digits: &str) -> Result<TmpfsMode, String> {
        let octal = (3..=4).contains(&digits.len()) && digits.chars().all(|c| ('0'..='7').contains(&c));
        u32::from_str_radix(digits, 8)
            .ok()
            .filter(|_| octal)
            .map(TmpfsMode)
            .ok_or_else(|| format!("`{digits}` isn't a mode: three or four octal digits, like \"1777\""))
    }
}

impl std::fmt::Display for TmpfsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04o}", self.0)
    }
}

impl Serialize for TmpfsMode {
    /// The mode as a spec writes it, `"1777"`.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TmpfsMode {
    /// Octal digits as text (`"1777"`), or the number YAML reads them as (`1777`).
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<TmpfsMode, D::Error> {
        d.deserialize_any(crate::syntax::Text("octal digits, like \"1777\""))?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for TmpfsMode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TmpfsMode".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": ["string", "integer"], "pattern": "^[0-7]{3,4}$" })
    }
}

impl Tmpfs {
    /// The absolute path in the machine.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Tmpfs::Path(p) => p,
            Tmpfs::Sized(t) => &t.path,
        }
    }

    /// Its size, when given.
    #[must_use]
    pub fn size(&self) -> Option<&str> {
        match self {
            Tmpfs::Path(_) => None,
            Tmpfs::Sized(t) => t.size.as_deref(),
        }
    }

    /// Its mode, when given.
    #[must_use]
    pub fn mode(&self) -> Option<TmpfsMode> {
        match self {
            Tmpfs::Path(_) => None,
            Tmpfs::Sized(t) => t.mode,
        }
    }
}

impl From<&str> for Tmpfs {
    fn from(path: &str) -> Tmpfs {
        Tmpfs::Path(path.to_owned())
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
                Ok(Tmpfs::Path(v.to_owned()))
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
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Filesystem::Ext4 => "ext4",
            Filesystem::Xfs => "xfs",
            Filesystem::Ntfs => "ntfs",
            Filesystem::Raw => "raw",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Why `yaml` doesn't read as a `T`.
    fn refusal<T: serde::de::DeserializeOwned>(yaml: &str) -> String {
        serde_yaml_ng::from_str::<T>(yaml).map(|_| ()).err().map(|e| e.to_string()).unwrap_or_default()
    }

    #[test]
    fn a_mount_is_a_path_or_a_folder_of_a_shared_volume() -> Result<(), serde_yaml_ng::Error> {
        let whole: Mount = serde_yaml_ng::from_str("/srv")?;
        assert_eq!((whole.path(), whole.subpath()), ("/srv", None));
        let part: Mount = serde_yaml_ng::from_str("{ path: /srv/www, subpath: www }")?;
        assert_eq!((part.path(), part.subpath()), ("/srv/www", Some("www")));
        assert!(refusal::<Mount>("3").ends_with("expected an absolute path, or `{ path, subpath }`"));
        Ok(())
    }

    #[test]
    fn a_tmpfs_is_a_path_or_one_with_its_size_and_mode() -> Result<(), serde_yaml_ng::Error> {
        let bare: Tmpfs = serde_yaml_ng::from_str("/run")?;
        assert_eq!((bare.path(), bare.size(), bare.mode()), ("/run", None, None));
        let sized: Tmpfs = serde_yaml_ng::from_str("{ path: /tmp, size: 64m, mode: \"1777\" }")?;
        assert_eq!((sized.path(), sized.size()), ("/tmp", Some("64m")));
        assert_eq!(sized.mode().map(TmpfsMode::bits), Some(0o1777));
        assert!(refusal::<Tmpfs>("3").ends_with("expected an absolute path, or `{ path, size, mode }`"));
        // Read once: a mode that isn't one never reaches a target.
        for bad in ["\"888\"", "\"8\"", "\"17777\"", "rwx"] {
            assert!(
                refusal::<Tmpfs>(&format!("{{ path: /tmp, mode: {bad} }}")).contains("isn't a mode: three or four octal digits"),
                "{bad}"
            );
        }
        let unquoted: Tmpfs = serde_yaml_ng::from_str("{ path: /tmp, mode: 755 }")?;
        assert_eq!(unquoted.mode().map(TmpfsMode::bits), Some(0o755));
        Ok(())
    }

    #[test]
    fn a_filesystem_goes_by_the_name_a_spec_writes() -> Result<(), serde_yaml_ng::Error> {
        for fs in [Filesystem::Ext4, Filesystem::Xfs, Filesystem::Ntfs, Filesystem::Raw] {
            assert_eq!(serde_yaml_ng::from_str::<Filesystem>(fs.id())?, fs);
        }
        Ok(())
    }
}
