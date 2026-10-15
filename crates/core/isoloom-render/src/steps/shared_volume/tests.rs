use isoloom_plan::lifecycle::SharedRole;

use super::linux;

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
    assert!(
        sh.contains("\nmkdir -p '/srv/shared/'\nmkdir -p '/srv/shared/www'\nif command -v apt-get"),
        "{sh}"
    );
    assert!(
        sh.contains("printf '%s %s %s\\n' '/srv/shared/' '10.0.0.11(rw,sync,no_subtree_check,no_root_squash) 10.0.0.12(rw,sync,no_subtree_check,no_root_squash)' '# isoloom-volume-shared'"),
        "{sh}"
    );
    assert!(sh.ends_with("exportfs -ra\n"), "{sh}");
}

#[test]
fn another_machine_mounts_the_export_and_waits_for_its_owner() {
    let role = SharedRole::Mount {
        source: "10.0.0.10:/srv/shared".into(),
        owner: "writer".into(),
    };
    let sh = linux("shared", "/data", &role);
    assert!(
        sh.contains("printf '%s\\n' '10.0.0.10:/srv/shared /data nfs defaults,_netdev 0 0' >> /etc/fstab"),
        "{sh}"
    );
    assert!(
        sh.contains("if [ $i -ge 180 ]; then echo 'volume shared: writer doesn'\\''t serve it (NFS)' >&2; exit 1; fi"),
        "{sh}"
    );
}

#[test]
fn the_whole_volume_is_mounted_by_its_tag() {
    let sh = linux("shared", "/srv/shared", &host_folder(None));
    assert!(sh.starts_with("mkdir -p '/srv/shared'\ntag=$(cat '/etc/isoloom/virtiofs/shared')\n"), "{sh}");
    assert!(sh.contains("virtiofs defaults,nofail 0 0"), "{sh}");
    assert!(sh.contains("mountpoint -q '/srv/shared' || mount '/srv/shared'"), "{sh}");
    assert!(!sh.contains("bind"), "{sh}");
}

#[test]
fn a_folder_is_bound_from_the_whole_volume() {
    let sh = linux("shared", "/var/www", &host_folder(Some("www")));
    assert!(sh.starts_with("mkdir -p '/var/lib/isoloom/volumes/shared' '/var/www'\n"), "{sh}");
    assert!(sh.contains("mount '/var/lib/isoloom/volumes/shared'"), "{sh}");
    assert!(sh.contains("mkdir -p '/var/lib/isoloom/volumes/shared/www'"), "{sh}");
    assert!(
        sh.contains("none bind,nofail,x-systemd.requires-mounts-for=/var/lib/isoloom/volumes/shared"),
        "{sh}"
    );
    assert!(sh.ends_with("mountpoint -q '/var/www' || mount '/var/www'\n"), "{sh}");
}
