//! Where machines keep data: volumes kept past `down` or existing already, a folder of a shared
//! volume, sized tmpfs mounts and data disks, each mistake named at its field.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // a failing test is the report.

use isoloom_spec::model::{Filesystem, Mount, Tmpfs};
use isoloom_spec::{parse, validate};

/// One network and two machines, `web` and `app`, with `extra` top-level lines and `web` and
/// `app` completed by their own fields.
fn spec(extra: &str, web: &str, app: &str) -> String {
    format!(
        "version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\n{extra}machines:\n  web: {{ networks: {{ lab: 5 }}, docker: {{ image: nginx }}{web} }}\n  app: {{ networks: {{ lab: 6 }}, docker: {{ image: alpine }}, vm: {{ os: debian-12, provision: [setup.sh] }}{app} }}\n"
    )
}

fn problems(yaml: &str) -> Vec<String> {
    let s = parse(yaml).unwrap_or_else(|e| panic!("parses: {e}\n{yaml}"));
    validate(&s).into_iter().map(|p| p.to_string()).collect()
}

#[track_caller]
fn assert_problem(yaml: &str, at: &str, fragment: &str) {
    let found = problems(yaml);
    assert!(
        found.iter().any(|p| p.starts_with(&format!("{at}: ")) && p.contains(fragment)),
        "expected `{at}: …{fragment}…`, got {found:#?}"
    );
}

#[test]
fn every_form_parses_and_a_valid_spec_has_no_problem() {
    let yaml = spec(
        "volumes: { site: { owner: web, persistent: true }, cache: { external: shared-cache } }\n",
        ", volumes: { site: /srv, cache: /cache }, tmpfs: [/tmp, { path: /run, size: 64m, mode: '1777' }], disks: { data: { size_gb: 5, mount: /data } }",
        ", volumes: { site: { path: /www, subpath: www/html } }, disks: { blk: { size_gb: 1, fs: raw } }",
    );
    assert_eq!(problems(&yaml), Vec::<String>::new());
    let s = parse(&yaml).unwrap();
    let app = &s.machines["app"];
    assert_eq!(app.volumes["site"].subpath(), Some("www/html"));
    assert_eq!(s.machines["web"].volumes["site"], Mount::from("/srv"));
    assert_eq!(s.machines["web"].tmpfs[0], Tmpfs::from("/tmp"));
    assert_eq!(s.machines["web"].tmpfs[1].mode(), Some("1777"));
    assert_eq!(app.disks["blk"].fs, Some(Filesystem::Raw));
}

#[test]
fn a_volume_is_kept_or_existing_not_both() {
    let yaml = spec(
        "volumes: { site: { persistent: true, external: 'bad name' } }\n",
        ", volumes: { site: /srv }",
        "",
    );
    assert_problem(&yaml, "volumes.site.persistent", "never removed");
    assert_problem(&yaml, "volumes.site.external", "isn't a volume name");
}

#[test]
fn only_a_shared_volume_has_folders_and_not_for_its_owner() {
    let own = spec("", ", volumes: { data: { path: /d, subpath: x } }", "");
    assert_problem(&own, "machines.web.volumes.data.subpath", "only a shared volume");
    let owner = spec(
        "volumes: { site: {} }\n",
        ", volumes: { site: { path: /d, subpath: x } }",
        ", volumes: { site: /s }",
    );
    assert_problem(&owner, "machines.web.volumes.site.subpath", "holds the whole volume");
    let escape = spec(
        "volumes: { site: {} }\n",
        ", volumes: { site: /d }",
        ", volumes: { site: { path: /s, subpath: ../etc } }",
    );
    assert_problem(&escape, "machines.app.volumes.site.subpath", "no `..`");
}

#[test]
fn a_tmpfs_size_and_mode_are_checked() {
    let yaml = spec("", ", tmpfs: [{ path: /run, size: 64mb, mode: '999' }]", "");
    assert_problem(&yaml, "machines.web.tmpfs[0].size", "isn't a size");
    assert_problem(&yaml, "machines.web.tmpfs[0].mode", "octal");
}

#[test]
fn a_disk_has_a_size_a_filesystem_for_its_os_and_its_own_mount() {
    let yaml = spec(
        "",
        ", volumes: { data: /data }, disks: { data: { size_gb: 1, mount: /x }, big: { size_gb: 0, mount: /data }, nt: { size_gb: 1, fs: ntfs, mount: /n } }",
        ", disks: { raw: { size_gb: 1, fs: raw, mount: /r }, bare: { size_gb: 1 }, a-very-long-name: { size_gb: 1, mount: /l } }",
    );
    assert_problem(&yaml, "machines.web.disks.data", "is a volume of this machine too");
    assert_problem(&yaml, "machines.web.disks.big.size_gb", "1 to 16384 GB");
    assert_problem(&yaml, "machines.web.disks.big.mount", "already a volume or a disk");
    assert_problem(&yaml, "machines.web.disks.nt.fs", "NTFS is a Windows machine's");
    assert_problem(&yaml, "machines.app.disks.raw.mount", "leave `mount` out");
    assert_problem(&yaml, "machines.app.disks.bare.mount", "say where");
    assert_problem(&yaml, "machines.app.disks.a-very-long-name", "at most 12 characters");
}

#[test]
fn a_windows_disk_mounts_at_a_drive_letter() {
    let yaml = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  dc: { networks: { lab: 5 }, vm: { os: windows-server-2022 }, disks: { logs: { size_gb: 1, mount: /logs }, sys: { size_gb: 1, mount: 'C:' }, ok: { size_gb: 1, mount: 'E:' }, x: { size_gb: 1, fs: xfs, mount: 'F:' } } }\n";
    assert_problem(yaml, "machines.dc.disks.logs.mount", "drive letter");
    assert_problem(yaml, "machines.dc.disks.sys.mount", "drive letter");
    assert_problem(yaml, "machines.dc.disks.x.fs", "formats NTFS");
    assert!(!problems(yaml).iter().any(|p| p.starts_with("machines.dc.disks.ok")));
}
