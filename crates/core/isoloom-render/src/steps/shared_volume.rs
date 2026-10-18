//! A shared volume on a VM ([`Step::SharedVolume`](isoloom_plan::Step::SharedVolume)): its owner
//! serves the directory over NFS to the machines mounting it, and each of them mounts it at its
//! own path (at boot too), waiting up to 15 minutes for the owner.
//!
//! A folder of the server (virtiofs on Proxmox) is mounted by every machine alike instead, by
//! its tag, at boot too, from `/etc/fstab`. A machine taking one folder of the volume
//! (`subpath`) mounts the whole of it out of the way, then binds its folder at its path, as no
//! virtiofs mount takes a folder of its own.

use std::net::Ipv4Addr;

use isoloom_plan::lifecycle::SharedRole;
use isoloom_spec::shell::quote;

use super::PKG;

/// Where a machine taking one folder of a volume mounts the whole of it.
const STAGING: &str = "/var/lib/isoloom/volumes";

/// Machine's part of shared volume `volume`, at `path`.
pub(super) fn linux(volume: &str, path: &str, role: &SharedRole) -> String {
    match role {
        SharedRole::Serve { clients, folders } => serve(volume, path, clients, folders),
        SharedRole::Mount { source, owner } => mount(volume, path, source, owner),
        SharedRole::HostFolder { tag_file, folder } => host_folder(volume, path, tag_file, folder.as_deref()),
    }
}

/// The owner: the folders others take made, the directory exported to their addresses.
fn serve(volume: &str, path: &str, clients: &[Ipv4Addr], folders: &[String]) -> String {
    let made: String = folders
        .iter()
        .map(|f| format!("mkdir -p {}\n", quote(&format!("{}/{f}", path.trim_end_matches('/')))))
        .collect::<Vec<_>>()
        .concat();
    let clients = clients
        .iter()
        .map(|c| format!("{c}(rw,sync,no_subtree_check,no_root_squash)"))
        .collect::<Vec<_>>()
        .join(" ");
    let tag = quote(&format!("# isoloom-volume-{volume}"));
    format!(
        "{PKG}\nmkdir -p {p}\n{made}if command -v apt-get >/dev/null; then pkg nfs-kernel-server; else pkg nfs-utils; fi\n\
         grep -v -F {tag} /etc/exports > /etc/exports.isoloom 2>/dev/null || true\n\
         printf '%s %s %s\\n' {p} {} {tag} >> /etc/exports.isoloom && mv /etc/exports.isoloom /etc/exports\n\
         systemctl enable --now nfs-server 2>/dev/null || systemctl enable --now nfs-kernel-server\n\
         exportfs -ra\n",
        quote(&clients),
        p = quote(path),
    )
}

/// Another machine: the owner's export in `/etc/fstab` once, mounted when the owner serves it.
fn mount(volume: &str, path: &str, source: &str, owner: &str) -> String {
    let fstab = format!("{source} {path} nfs defaults,_netdev 0 0");
    format!(
        "{PKG}\nmkdir -p {p}\nif command -v apt-get >/dev/null; then pkg nfs-common; else pkg nfs-utils; fi\n\
         grep -qF {} /etc/fstab || printf '%s\\n' {} >> /etc/fstab\n\
         i=0; until mountpoint -q {p} || mount {p} 2>/dev/null; do i=$((i+1)); if [ $i -ge 180 ]; then echo {} >&2; exit 1; fi; sleep 5; done\n",
        quote(&format!("{source} ")),
        quote(&fstab),
        quote(&format!("volume {volume}: {owner} does not serve it (NFS)")),
        p = quote(path),
    )
}

/// A folder of the server: the tag read from `tag_file`, the mount (and the bind for a folder)
/// in `/etc/fstab` once, then mounted now.
fn host_folder(volume: &str, path: &str, tag_file: &str, folder: Option<&str>) -> String {
    let whole = match folder {
        Some(_) => format!("{STAGING}/{volume}"),
        None => path.to_owned(),
    };
    let failed = quote(&format!(
        "volume {volume}: the server's folder isn't shared into this VM (virtiofs): does the mapping's folder exist on the node?"
    ));
    let dirs = if whole == path {
        quote(path)
    } else {
        format!("{} {}", quote(&whole), quote(path))
    };
    let mounted = format!(
        "mkdir -p {dirs}\ntag=$(cat {})\ngrep -qF {} /etc/fstab || printf '%s %s virtiofs defaults,nofail 0 0\\n' \"$tag\" {w} >> /etc/fstab\n\
         mountpoint -q {w} || mount {w} || {{ echo {failed} >&2; exit 1; }}\n",
        quote(tag_file),
        quote(&format!(" {whole} virtiofs ")),
        w = quote(&whole),
    );
    let bound = folder.map(|f| {
        let source = format!("{whole}/{f}");
        let line = format!("{source} {path} none bind,nofail,x-systemd.requires-mounts-for={whole} 0 0");
        format!(
            "mkdir -p {}\ngrep -qF {} /etc/fstab || printf '%s\\n' {} >> /etc/fstab\nmountpoint -q {p} || mount {p}\n",
            quote(&source),
            quote(&format!("{source} {path} none ")),
            quote(&line),
            p = quote(path),
        )
    });
    mounted + &bound.unwrap_or_default()
}

#[cfg(test)]
mod tests;
