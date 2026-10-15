//! A machine's storage beyond its root disk, as every target reads it: how long a volume lives
//! (with the environment, past it, or already there), its data disks with their filesystem
//! resolved for its OS, and what that storage asks of a backend.

use isoloom_spec::images;
use isoloom_spec::model::{Filesystem, Machine, Spec};

use crate::requirements::Requirement;

/// How long a volume lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifetime {
    /// Created with the environment, removed by `down`.
    Environment,
    /// Created with the environment, kept by `down` (removed by `down --volumes`).
    Persistent,
    /// Already there: mounted by this name, never created or removed.
    External {
        /// Its name on the platform.
        name: String,
    },
}

/// The lifetime of the storage behind machine volume `volume`: a top-level volume's own, else
/// the environment's.
#[must_use]
pub fn lifetime(spec: &Spec, volume: &str) -> Lifetime {
    let Some(v) = spec.volumes.get(volume) else {
        return Lifetime::Environment;
    };
    match &v.external {
        Some(name) => Lifetime::External { name: name.clone() },
        None if v.persistent => Lifetime::Persistent,
        None => Lifetime::Environment,
    }
}

/// A data disk of a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDisk {
    /// Its name in the spec (the filesystem's label).
    pub name: String,
    /// The name the platform knows it by: `<machine>-<disk>`.
    pub key: String,
    /// Its size, in GB.
    pub size_gb: u32,
    /// Its filesystem, the OS's default when the spec leaves it out.
    pub fs: Filesystem,
    /// Where it is mounted (a path, or a drive letter on Windows); `None` for a raw disk.
    pub mount: Option<String>,
}

impl DataDisk {
    /// Whether it is a block device left unformatted.
    #[must_use]
    pub fn is_raw(&self) -> bool {
        self.fs == Filesystem::Raw
    }

    /// Its size in bytes, as the OS sees it (GiB: what every platform's GB is).
    #[must_use]
    pub fn bytes(&self) -> u64 {
        u64::from(self.size_gb) << 30
    }
}

/// Machine `name`'s data disks, in spec order.
#[must_use]
pub fn disks(name: &str, m: &Machine) -> Vec<DataDisk> {
    let windows = m.vm.as_ref().is_some_and(|vm| images::is_windows(&vm.os));
    let default = if windows { Filesystem::Ntfs } else { Filesystem::Ext4 };
    m.disks
        .iter()
        .map(|(d, disk)| DataDisk {
            name: d.clone(),
            key: format!("{name}-{d}"),
            size_gb: disk.size_gb,
            fs: disk.fs.unwrap_or(default),
            mount: disk.mount.clone(),
        })
        .collect()
}

/// What the spec's storage asks of a backend: volumes kept past `down` or existing already,
/// data disks (raw ones apart), tmpfs modes. At most one of each kind: the first that asks.
#[must_use]
pub fn requirements(spec: &Spec) -> Vec<Requirement> {
    let mut out = Vec::new();
    if let Some((v, _)) = spec.volumes.iter().find(|(_, v)| v.persistent) {
        out.push(Requirement::PersistentVolume { volume: v.clone() });
    }
    if let Some((v, _)) = spec.volumes.iter().find(|(_, v)| v.external.is_some()) {
        out.push(Requirement::ExternalVolume { volume: v.clone() });
    }
    if let Some((m, _)) = spec.machines.iter().find(|(_, m)| !m.disks.is_empty()) {
        out.push(Requirement::DataDisks { machine: m.clone() });
    }
    let raw = |m: &Machine| m.disks.values().any(|d| d.fs == Some(Filesystem::Raw));
    if let Some((m, _)) = spec.machines.iter().find(|(_, m)| raw(m)) {
        out.push(Requirement::RawDisks { machine: m.clone() });
    }
    if let Some((m, _)) = spec.machines.iter().find(|(_, m)| m.tmpfs.iter().any(|t| t.mode().is_some())) {
        out.push(Requirement::TmpfsModes { machine: m.clone() });
    }
    out
}

/// Why a target whose storage goes with its machines refuses persistent volume `volume`.
pub(crate) fn no_persistent_volume(f: &mut std::fmt::Formatter<'_>, volume: &str) -> std::fmt::Result {
    write!(
        f,
        "volume `{volume}` is persistent: it outlives `down` on Docker and Kubernetes, while this target's storage goes with its machines"
    )
}

/// Why a target with nothing to mount by name refuses existing volume `volume`.
pub(crate) fn no_existing_volume(f: &mut std::fmt::Formatter<'_>, volume: &str) -> std::fmt::Result {
    write!(
        f,
        "volume `{volume}` is an existing one (`external`): Docker and Kubernetes mount one by name, this target has none to mount"
    )
}

/// Why a target that attaches no disk refuses machine `machine`'s.
pub(crate) fn no_data_disks(f: &mut std::fmt::Formatter<'_>, machine: &str) -> std::fmt::Result {
    write!(f, "machine `{machine}` has data disks (`disks`), which this target can't attach")
}

/// Why a target whose containers get no block device refuses machine `machine`'s raw disk.
pub(crate) fn no_raw_disks(f: &mut std::fmt::Formatter<'_>, machine: &str) -> std::fmt::Result {
    write!(
        f,
        "machine `{machine}` has a raw disk (`fs: raw`): a block device, which this target's containers don't get"
    )
}

/// Why Kubernetes refuses machine `machine`'s tmpfs mode.
pub(crate) fn no_tmpfs_modes(f: &mut std::fmt::Formatter<'_>, machine: &str) -> std::fmt::Result {
    write!(f, "machine `{machine}` sets a tmpfs `mode`: Kubernetes' memory volumes have none")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disk_takes_its_os_default_filesystem_and_a_key_of_its_machine() {
        let spec = crate::testing::spec(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  db: { networks: { lan: 10 }, os: debian@12, disks: { data: { size_gb: 5, mount: /data }, blk: { size_gb: 1, fs: raw } } }\n  win: { networks: { lan: 11 }, os: windows-server@2022, disks: { logs: { size_gb: 2, mount: 'E:' } } }\n",
        );
        let db = disks("db", &spec.machines["db"]);
        assert_eq!((db[0].key.as_str(), db[0].fs, db[0].bytes()), ("db-data", Filesystem::Ext4, 5 << 30));
        assert!(db[1].is_raw());
        assert_eq!(disks("win", &spec.machines["win"])[0].fs, Filesystem::Ntfs);
        let kinds: Vec<_> = requirements(&spec).iter().map(Requirement::feature).collect();
        assert_eq!(kinds, [crate::Feature::DataDisks, crate::Feature::RawDisks]);
    }
}
