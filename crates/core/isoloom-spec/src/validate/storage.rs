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
        let drive_letter = matches!(m.as_bytes(), [letter, b':'] if (b'D'..=b'Z').contains(&letter.to_ascii_uppercase()));
        if !drive_letter {
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
    use crate::testing::{lab, problems};

    /// The problems of machine `name` in form `form` (its `docker:` or `vm:`) with `storage`.
    fn named(name: &str, form: &str, storage: &str) -> Vec<String> {
        problems(&lab(&format!("machines:\n  {name}: {{ networks: {{ lan: 10 }}, {form}, {storage} }}\n")))
    }

    /// The problems of a Debian VM `web` with `disks`.
    fn linux(disks: &str) -> Vec<String> {
        named("web", "vm: { os: debian@12 }", &format!("disks: {disks}"))
    }

    /// The problems of a Windows VM `web` with `disks`.
    fn windows(disks: &str) -> Vec<String> {
        named("web", "vm: { os: windows-server@2022 }", &format!("disks: {disks}"))
    }

    #[test]
    fn sizes_and_modes() {
        assert!(size_ok("64m") && size_ok("1048576") && size_ok("2G"));
        assert!(!size_ok("m") && !size_ok("0") && !size_ok("64mb") && !size_ok("-1"));
        assert!(mode_ok("1777") && mode_ok("700") && mode_ok("0755"));
        assert!(!mode_ok("8") && !mode_ok("17777") && !mode_ok("rwx"));
    }

    #[test]
    fn a_tmpfs_mount_has_a_safe_path_a_size_and_a_mode() {
        let tmpfs = |t: &str| named("web", "docker: { image: nginx }", &format!("tmpfs: [{t}]"));
        assert_eq!(tmpfs("/run, { path: /cache, size: 64m, mode: '1777' }"), Vec::<String>::new());
        assert_eq!(
            tmpfs("run"),
            ["machines.web.tmpfs[0]: `run` isn't a safe absolute path inside the machine, like /run"]
        );
        assert_eq!(
            tmpfs("{ path: /cache, size: 64mb }"),
            ["machines.web.tmpfs[0].size: `64mb` isn't a size: bytes, or a number with k, m or g, like 64m"]
        );
        assert_eq!(
            tmpfs("{ path: /cache, mode: '888' }"),
            ["machines.web.tmpfs[0].mode: `888` isn't a mode: three or four octal digits, like \"1777\""]
        );
    }

    #[test]
    fn a_data_disk_is_1_gb_to_16_tib() {
        let sized = |gb: u32| linux(&format!("{{ data: {{ size_gb: {gb}, fs: ext4, mount: /data }} }}"));
        assert_eq!((sized(1), sized(16_384)), (Vec::<String>::new(), Vec::<String>::new()));
        let wrong = "machines.web.disks.data.size_gb: a data disk is 1 to 16384 GB";
        assert_eq!(sized(0), [wrong]);
        assert_eq!(sized(16_385), [wrong]);
    }

    #[test]
    fn a_disk_name_is_its_filesystems_label() {
        let disk = |d: &str| linux(&format!("{{ {d}: {{ size_gb: 1, mount: /data }} }}"));
        assert_eq!(disk("abcdefghij-l"), Vec::<String>::new());
        for wrong in ["abcdefghij-lm", "Data"] {
            assert_eq!(
                disk(wrong),
                [format!(
                    "machines.web.disks.{wrong}: disk names are kebab-case, at most 12 characters: the filesystem's label is the name"
                )],
                "{wrong}"
            );
        }
    }

    #[test]
    fn a_machine_and_disk_name_fit_a_kubernetes_claim() {
        let on = |machine: &str| named(machine, "vm: { os: debian@12 }", "disks: { abcdefghijkl: { size_gb: 1, mount: /data } }");
        assert_eq!(on(&"m".repeat(50)), Vec::<String>::new());
        let long = "m".repeat(51);
        assert_eq!(
            on(&long),
            [format!(
                "machines.{long}.disks.abcdefghijkl: machine name and disk name together are at most 62 characters: the Kubernetes claim is `<machine>-<disk>`"
            )]
        );
    }

    #[test]
    fn a_disk_takes_the_filesystem_its_os_formats() {
        assert_eq!(
            linux("{ a: { size_gb: 1, fs: xfs, mount: /a }, b: { size_gb: 1, fs: raw } }"),
            Vec::<String>::new()
        );
        assert_eq!(
            windows("{ a: { size_gb: 1, fs: ntfs, mount: 'E:' }, b: { size_gb: 1, mount: 'f:' } }"),
            Vec::<String>::new()
        );
        assert_eq!(
            linux("{ a: { size_gb: 1, fs: ntfs, mount: /a } }"),
            ["machines.web.disks.a.fs: NTFS is a Windows machine's: use ext4 or xfs"]
        );
        let ntfs = "machines.web.disks.a.fs: a Windows machine formats NTFS: use ntfs, or leave it out";
        assert_eq!(windows("{ a: { size_gb: 1, fs: ext4, mount: 'E:' } }"), [ntfs]);
        assert_eq!(windows("{ a: { size_gb: 1, fs: xfs, mount: 'E:' } }"), [ntfs]);
    }

    #[test]
    fn a_disk_with_a_filesystem_is_mounted_and_a_raw_one_is_not() {
        assert_eq!(
            linux("{ a: { size_gb: 1, fs: raw, mount: /a } }"),
            ["machines.web.disks.a.mount: a raw disk has no filesystem to mount: leave `mount` out"]
        );
        assert_eq!(
            linux("{ a: { size_gb: 1 } }"),
            ["machines.web.disks.a.mount: say where the disk is mounted, like /data (or make it `fs: raw`)"]
        );
    }

    #[test]
    fn a_mount_is_a_safe_path_on_linux_and_a_data_drive_on_windows() {
        assert_eq!(
            linux("{ a: { size_gb: 1, mount: data } }"),
            ["machines.web.disks.a.mount: `data` isn't a safe absolute path inside the machine, like /data"]
        );
        for wrong in ["C:", "E:/", "/data", "EE"] {
            assert_eq!(
                windows(&format!("{{ a: {{ size_gb: 1, mount: '{wrong}' }} }}")),
                [format!(
                    "machines.web.disks.a.mount: `{wrong}` isn't a drive letter: a Windows disk mounts at one, D: to Z:"
                )],
                "{wrong}"
            );
        }
        assert_eq!(windows("{ a: { size_gb: 1, mount: 'Z:' } }"), Vec::<String>::new());
    }

    #[test]
    fn a_mount_point_holds_one_volume_or_disk() {
        let taken = |what: &str| [format!("machines.web.disks.b.mount: `{what}` is already a volume or a disk of this machine")];
        assert_eq!(linux("{ a: { size_gb: 1, mount: /data }, b: { size_gb: 1, mount: /data/ } }"), taken("/data/"));
        assert_eq!(windows("{ a: { size_gb: 1, mount: 'E:' }, b: { size_gb: 1, mount: 'e:' } }"), taken("e:"));
        assert_eq!(
            named(
                "web",
                "vm: { os: debian@12 }",
                "volumes: { logs: /data }, disks: { b: { size_gb: 1, mount: /data } }"
            ),
            taken("/data")
        );
    }
}
