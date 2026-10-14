//! The options of `config.vm.disk` and `config.vm.synced_folder`.

use super::{DEVICES, FOLDERS, HOST_TUNING, MORE_DISKS, pairs, table};
use crate::{
    Format, Support,
    Support::{Emitted, Equivalent, NotPortable, Tooling},
};

/// The "Vagrant: disks and synced folders" table.
pub(super) fn format() -> Format {
    let keys = pairs(FOLDERS)
        .into_iter()
        .map(|(kind, o)| {
            let s = match kind {
                "disk" => disk(o),
                "synced_folder" => synced_folder(o),
                _ => None,
            };
            (format!("config.vm.{kind}: {o}"), s)
        })
        .collect();
    table(
        "Vagrant: disks and synced folders",
        "Vagrant 2.4.9 (the disk config class and the synced folder docs)",
        keys,
    )
}

fn disk(o: &str) -> Option<Support> {
    Some(match o {
        "size" => Emitted {
            from: "machines.*.resources.disk_gb",
        },
        "type" | "primary" => Emitted {
            from: "(the VM's own disk, `:disk` and primary, grown to `disk_gb`)",
        },
        "name" => MORE_DISKS,
        "file" => DEVICES,
        "disk_ext" | "provider_config" => HOST_TUNING,
        "id" => Tooling {
            note: "Vagrant's own identifier for the disk",
        },
        _ => return None,
    })
}

/// A live share of the user's folder: a cloud VM or a Proxmox server can't mount it.
const LIVE_SHARE: Support = NotPortable {
    why: "a live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead)",
};

fn synced_folder(o: &str) -> Option<Support> {
    Some(match o {
        "disabled" => Emitted {
            from: "(the default `/vagrant` share, turned off)",
        },
        o if o.starts_with("rsync__") => Equivalent {
            via: "the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out)",
        },
        "type" | "create" | "owner" | "group" | "mount_options" | "id" => LIVE_SHARE,
        o if o.starts_with("nfs_") || o.starts_with("smb_") => LIVE_SHARE,
        _ => return None,
    })
}
