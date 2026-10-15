//! A machine's storage besides its volumes: its memory-backed mounts (a size and a mode each
//! runtime reads) and its data disks (a size the platforms take, a filesystem its OS formats, a
//! mount point of its own).

use std::collections::BTreeSet;

use super::machines::Subject;
use super::volumes::safe_path;
use super::{Problems, kebab};
use crate::model::{Disk, Filesystem};

/// The largest data disk, in GB: what every cloud's block storage takes (16 TiB).
const MAX_DISK_GB: u32 = 16_384;

/// The longest disk name: the filesystem's label is the disk's name, and XFS takes 12.
const MAX_DISK_NAME: usize = 12;

/// The machine's tmpfs mounts and data disks. `volume_paths` are its volumes' paths, which a disk
/// can't take.
pub(super) fn of_machine(s: &Subject, volume_paths: &BTreeSet<&str>, p: &mut Problems) {
    for (i, t) in s.m.tmpfs.iter().enumerate() {
        let at = format!("{}.tmpfs[{i}]", s.at);
        if !safe_path(t.path()) {
            p.add(&at, format!("`{}` isn't a safe absolute path inside the machine, like /run", t.path()));
        }
        if let Some(size) = t.size().filter(|z| !size_ok(z)) {
            p.add(
                format!("{at}.size"),
                format!("`{size}` isn't a size: bytes, or a number with k, m or g, like 64m"),
            );
        }
        if let Some(mode) = t.mode().filter(|m| !mode_ok(m)) {
            p.add(
                format!("{at}.mode"),
                format!("`{mode}` isn't a mode: three or four octal digits, like \"1777\""),
            );
        }
    }
    let mut mounts = BTreeSet::new();
    for (name, d) in &s.m.disks {
        let at = format!("{}.disks.{name}", s.at);
        if let Some(why) = disk_name_problem(s, name) {
            p.add(&at, why);
        }
        if d.size_gb == 0 || d.size_gb > MAX_DISK_GB {
            p.add(format!("{at}.size_gb"), format!("a data disk is 1 to {MAX_DISK_GB} GB"));
        }
        let windows = s.m.vm.as_ref().is_some_and(|vm| crate::images::is_windows(&vm.os));
        filesystem(d, windows, &at, p);
        if let Some(m) = &d.mount {
            mount(m, windows, &at, p);
            if volume_paths.contains(m.trim_end_matches('/')) || !mounts.insert(m.trim_end_matches('/').to_ascii_uppercase()) {
                p.add(format!("{at}.mount"), format!("`{m}` is already a volume or a disk of this machine"));
            }
        }
    }
}

/// What is wrong with disk `name`'s name on this machine, if anything.
fn disk_name_problem(s: &Subject, name: &str) -> Option<String> {
    if !kebab(name) || name.len() > MAX_DISK_NAME {
        Some(format!(
            "disk names are kebab-case, at most {MAX_DISK_NAME} characters: the filesystem's label is the name"
        ))
    } else if s.m.volumes.contains_key(name) {
        Some(format!(
            "`{name}` is a volume of this machine too: a disk is stored as `<machine>-<disk>`, as a volume"
        ))
    } else if s.name.len() + 1 + name.len() > 63 {
        Some("machine name and disk name together are at most 62 characters: the Kubernetes claim is `<machine>-<disk>`".into())
    } else {
        None
    }
}

/// A filesystem the machine's OS formats: ext4 or XFS on Linux, NTFS on Windows, or none.
fn filesystem(d: &Disk, windows: bool, at: &str, p: &mut Problems) {
    match (d.fs, windows) {
        (Some(Filesystem::Ntfs), false) => p.add(format!("{at}.fs"), "NTFS is a Windows machine's: use ext4 or xfs"),
        (Some(Filesystem::Ext4 | Filesystem::Xfs), true) => p.add(format!("{at}.fs"), "a Windows machine formats NTFS: use ntfs, or leave it out"),
        _ => {}
    }
    match (d.fs == Some(Filesystem::Raw), d.mount.is_some()) {
        (true, true) => p.add(format!("{at}.mount"), "a raw disk has no filesystem to mount: leave `mount` out"),
        (false, false) => p.add(format!("{at}.mount"), "say where the disk is mounted, like /data (or make it `fs: raw`)"),
        _ => {}
    }
}

/// A mount point: a safe absolute path on Linux, a drive letter other than the system's on
/// Windows.
fn mount(m: &str, windows: bool, at: &str, p: &mut Problems) {
    if windows {
        let b = m.as_bytes();
        if !(b.len() == 2 && b[1] == b':' && (b'D'..=b'Z').contains(&b[0].to_ascii_uppercase())) {
            p.add(
                format!("{at}.mount"),
                format!("`{m}` isn't a drive letter: a Windows disk mounts at one, D: to Z:"),
            );
        }
    } else if !safe_path(m) {
        p.add(
            format!("{at}.mount"),
            format!("`{m}` isn't a safe absolute path inside the machine, like /data"),
        );
    }
}

/// A size: bytes, or a number with a unit (`k`, `m`, `g`, either case).
fn size_ok(size: &str) -> bool {
    let digits = size.strip_suffix(['k', 'm', 'g', 'K', 'M', 'G']).unwrap_or(size);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && !digits.trim_start_matches('0').is_empty()
}

/// A mode: three or four octal digits.
fn mode_ok(mode: &str) -> bool {
    (3..=4).contains(&mode.len()) && mode.chars().all(|c| ('0'..='7').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_modes() {
        assert!(size_ok("64m") && size_ok("1048576") && size_ok("2G"));
        assert!(!size_ok("m") && !size_ok("0") && !size_ok("64mb") && !size_ok("-1"));
        assert!(mode_ok("1777") && mode_ok("700") && mode_ok("0755"));
        assert!(!mode_ok("8") && !mode_ok("17777") && !mode_ok("rwx"));
    }
}
