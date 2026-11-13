//! The named volumes of the Compose file, and a service's mounts.

use std::ops::Not;

use indexmap::IndexMap;
use serde::Serialize;

/// A named volume: Isoloom's labels, or an existing volume's name.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Volume {
    /// Created outside this file (by `name`), not by Compose: never removed by it either.
    #[serde(skip_serializing_if = "Not::not")]
    pub(crate) external: bool,
    /// Its name on the Docker host, when not `<project>_<key>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    /// The volume's labels (Isoloom's own, to find what it created and what `down` keeps).
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) labels: IndexMap<String, String>,
}

/// One of a service's mounts: the short form (`source:target[:ro]`), or the long one for a
/// mount with options.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum Mount {
    /// `source:target[:ro]`.
    Short(String),
    /// A mount with its options.
    Long(LongMount),
}

impl From<String> for Mount {
    fn from(short: String) -> Mount {
        Mount::Short(short)
    }
}

/// A mount in the long form: a folder of a named volume, a tmpfs with options, or a folder of the
/// host.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum LongMount {
    /// A folder of a named volume.
    Volume {
        /// The named volume.
        source: String,
        /// The path in the container.
        target: String,
        /// The folder mounted.
        volume: VolumeOptions,
    },
    /// A folder of the host (a host folder).
    Bind {
        /// The folder on the host: absolute, `~`, or relative to the Compose file.
        source: String,
        /// The path in the container.
        target: String,
        /// The container only reads it.
        #[serde(skip_serializing_if = "Not::not")]
        read_only: bool,
        /// The folder created when missing.
        bind: BindOptions,
    },
    /// An in-memory file system with a size or a mode.
    Tmpfs {
        /// The path in the container.
        target: String,
        /// Its size and mode.
        tmpfs: TmpfsOptions,
    },
}

/// `volumes[].volume`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct VolumeOptions {
    /// The folder of the volume mounted instead of all of it.
    pub(crate) subpath: String,
}

/// The mode of a read-only bind of a project file (the short form's `:ro,z`): relabelled for
/// SELinux as shared (`z`), without which an enforcing host (Fedora, RHEL) denies the container
/// every read of it; nothing where SELinux is off or absent (Debian, Docker Desktop's VM).
pub(crate) const RO_BIND: &str = "ro,z";

/// The SELinux label option of a bind ([`RO_BIND`]'s `z`, the long form).
pub(crate) const SELINUX_SHARED: &str = "z";

/// `volumes[].bind`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct BindOptions {
    /// Create the folder when there is nothing at its path (the long form doesn't by default).
    pub(crate) create_host_path: bool,
    /// The SELinux relabelling (`z`: shared between containers), [`SELINUX_SHARED`].
    pub(crate) selinux: &'static str,
}

/// `volumes[].tmpfs`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct TmpfsOptions {
    /// Its most memory (`64m`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<String>,
    /// Its permissions, as a number (Compose reads the octal mode's value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_mount_says_its_type_first() {
        let folder = LongMount::Volume {
            source: "data".into(),
            target: "/srv".into(),
            volume: VolumeOptions { subpath: "web".into() },
        };
        let tmpfs = LongMount::Tmpfs {
            target: "/run".into(),
            tmpfs: TmpfsOptions {
                size: None,
                mode: Some(0o1777),
            },
        };
        let yaml = serde_yaml_ng::to_string(&[Mount::Long(folder), Mount::Long(tmpfs)]).expect("serializes");
        assert_eq!(
            yaml,
            "- type: volume\n  source: data\n  target: /srv\n  volume:\n    subpath: web\n- type: tmpfs\n  target: /run\n  tmpfs:\n    mode: 1023\n"
        );
    }
}
