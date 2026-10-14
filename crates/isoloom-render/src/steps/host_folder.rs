//! A shared volume as a folder of the server (virtiofs on Proxmox): every machine using it
//! mounts the folder by its tag, at boot too, from `/etc/fstab`. A machine taking one folder of
//! the volume (`subpath`) mounts the whole of it out of the way, then binds its folder at its
//! path, as no virtiofs mount takes a folder of its own.

use std::fmt::Write as _;

use crate::shell::quote;

/// Where a machine taking one folder of a volume mounts the whole of it.
const STAGING: &str = "/var/lib/isoloom/volumes";

/// The script: the tag read from `tag_file`, the mount (and the bind for a folder) in
/// `/etc/fstab` once, then mounted now.
pub(super) fn linux(volume: &str, path: &str, tag_file: &str, folder: Option<&str>) -> String {
    let whole = match folder {
        Some(_) => format!("{STAGING}/{volume}"),
        None => path.to_string(),
    };
    let failed = quote(&format!(
        "volume {volume}: the server's folder isn't shared into this VM (virtiofs): does the mapping's folder exist on the node?"
    ));
    let dirs = if whole == path {
        quote(path)
    } else {
        format!("{} {}", quote(&whole), quote(path))
    };
    let mut sh = format!("mkdir -p {dirs}\ntag=$(cat {})\n", quote(tag_file));
    let _ = writeln!(
        sh,
        "grep -qF {} /etc/fstab || printf '%s %s virtiofs defaults,nofail 0 0\\n' \"$tag\" {w} >> /etc/fstab\n\
         mountpoint -q {w} || mount {w} || {{ echo {failed} >&2; exit 1; }}",
        quote(&format!(" {whole} virtiofs ")),
        w = quote(&whole),
    );
    if let Some(f) = folder {
        let source = format!("{whole}/{f}");
        let line = format!("{source} {path} none bind,nofail,x-systemd.requires-mounts-for={whole} 0 0");
        let _ = writeln!(
            sh,
            "mkdir -p {}\ngrep -qF {} /etc/fstab || printf '%s\\n' {} >> /etc/fstab\nmountpoint -q {p} || mount {p}",
            quote(&source),
            quote(&format!("{source} {path} none ")),
            quote(&line),
            p = quote(path),
        );
    }
    sh
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_whole_volume_is_mounted_by_its_tag() {
        let sh = super::linux("shared", "/srv/shared", "/etc/isoloom/virtiofs/shared", None);
        assert!(sh.contains("tag=$(cat '/etc/isoloom/virtiofs/shared')"), "{sh}");
        assert!(sh.contains("virtiofs defaults,nofail 0 0"), "{sh}");
        assert!(sh.contains("mountpoint -q '/srv/shared' || mount '/srv/shared'"), "{sh}");
        assert!(!sh.contains("bind"), "{sh}");
    }

    #[test]
    fn a_folder_is_bound_from_the_whole_volume() {
        let sh = super::linux("shared", "/var/www", "/etc/isoloom/virtiofs/shared", Some("www"));
        assert!(sh.contains("mount '/var/lib/isoloom/volumes/shared'"), "{sh}");
        assert!(sh.contains("mkdir -p '/var/lib/isoloom/volumes/shared/www'"), "{sh}");
        assert!(
            sh.contains("none bind,nofail,x-systemd.requires-mounts-for=/var/lib/isoloom/volumes/shared"),
            "{sh}"
        );
        assert!(sh.contains("mountpoint -q '/var/www' || mount '/var/www'"), "{sh}");
    }
}
