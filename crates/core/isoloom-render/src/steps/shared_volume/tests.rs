use isoloom_plan::lifecycle::SharedRole;

use super::linux;

/// The lines of a step.
fn lines(sh: &str) -> Vec<&str> {
    sh.lines().collect()
}

fn host_folder(folder: Option<&str>) -> SharedRole {
    SharedRole::HostFolder {
        tag_file: "/etc/isoloom/virtiofs/shared".into(),
        folder: folder.map(Into::into),
    }
}

#[test]
fn the_owner_exports_the_folder_to_each_machine_mounting_it() {
    let role = SharedRole::Serve {
        clients: vec!["10.0.0.11".parse().unwrap(), "10.0.0.12".parse().unwrap()],
        folders: vec!["www".into()],
    };
    let sh = linux("shared", "/srv/shared/", &role);
    assert_eq!(
        lines(&sh),
        [
            super::super::PKG,
            "mkdir -p '/srv/shared/'",
            "mkdir -p '/srv/shared/www'",
            "if command -v apt-get >/dev/null; then pkg nfs-kernel-server; else pkg nfs-utils; fi",
            "grep -v -F '# isoloom-volume-shared' /etc/exports > /etc/exports.isoloom 2>/dev/null || true",
            "printf '%s %s %s\\n' '/srv/shared/' '10.0.0.11(rw,sync,no_subtree_check,no_root_squash) 10.0.0.12(rw,sync,no_subtree_check,no_root_squash)' '# isoloom-volume-shared' >> /etc/exports.isoloom && mv /etc/exports.isoloom /etc/exports",
            "systemctl enable --now nfs-server 2>/dev/null || systemctl enable --now nfs-kernel-server",
            "exportfs -ra"
        ],
        "the server installed, each folder made, the export replaced, then served"
    );
}

#[test]
fn another_machine_mounts_the_export_and_waits_for_its_owner() {
    let role = SharedRole::Mount {
        source: "10.0.0.10:/srv/shared".into(),
        owner: "writer".into(),
    };
    let sh = linux("shared", "/data", &role);
    assert_eq!(
        lines(&sh),
        [
            super::super::PKG,
            "mkdir -p '/data'",
            "if command -v apt-get >/dev/null; then pkg nfs-common; else pkg nfs-utils; fi",
            "grep -qF '10.0.0.10:/srv/shared ' /etc/fstab || printf '%s\\n' '10.0.0.10:/srv/shared /data nfs defaults,_netdev 0 0' >> /etc/fstab",
            "i=0; until mountpoint -q '/data' || mount '/data' 2>/dev/null; do i=$((i+1)); if [ $i -ge 180 ]; then echo 'volume shared: writer does not serve it (NFS)' >&2; exit 1; fi; sleep 5; done"
        ],
        "the client installed, the mount in fstab once, mounted once its owner serves it"
    );
}

#[test]
fn the_whole_volume_is_mounted_by_its_tag() {
    let sh = linux("shared", "/srv/shared", &host_folder(None));
    assert_eq!(
        lines(&sh),
        [
            "mkdir -p '/srv/shared'",
            "tag=$(cat '/etc/isoloom/virtiofs/shared')",
            "grep -qF ' /srv/shared virtiofs ' /etc/fstab || printf '%s %s virtiofs defaults,nofail 0 0\\n' \"$tag\" '/srv/shared' >> /etc/fstab",
            "mountpoint -q '/srv/shared' || mount '/srv/shared' || { echo 'volume shared: the server'\\''s folder isn'\\''t shared into this VM (virtiofs): does the mapping'\\''s folder exist on the node?' >&2; exit 1; }"
        ],
        "mounted by its tag, in fstab once, no bind"
    );
}

#[test]
fn a_folder_is_bound_from_the_whole_volume() {
    let sh = linux("shared", "/var/www", &host_folder(Some("www")));
    assert_eq!(
        lines(&sh),
        [
            "mkdir -p '/var/lib/isoloom/volumes/shared' '/var/www'",
            "tag=$(cat '/etc/isoloom/virtiofs/shared')",
            "grep -qF ' /var/lib/isoloom/volumes/shared virtiofs ' /etc/fstab || printf '%s %s virtiofs defaults,nofail 0 0\\n' \"$tag\" '/var/lib/isoloom/volumes/shared' >> /etc/fstab",
            "mountpoint -q '/var/lib/isoloom/volumes/shared' || mount '/var/lib/isoloom/volumes/shared' || { echo 'volume shared: the server'\\''s folder isn'\\''t shared into this VM (virtiofs): does the mapping'\\''s folder exist on the node?' >&2; exit 1; }",
            "mkdir -p '/var/lib/isoloom/volumes/shared/www'",
            "grep -qF '/var/lib/isoloom/volumes/shared/www /var/www none ' /etc/fstab || printf '%s\\n' '/var/lib/isoloom/volumes/shared/www /var/www none bind,nofail,x-systemd.requires-mounts-for=/var/lib/isoloom/volumes/shared 0 0' >> /etc/fstab",
            "mountpoint -q '/var/www' || mount '/var/www'"
        ],
        "the whole volume mounted, then the folder bound from it"
    );
}
