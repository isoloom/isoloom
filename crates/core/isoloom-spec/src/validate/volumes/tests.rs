use crate::testing::{lab, problems};

/// The problems of machine `name` mounting `volumes`.
fn mounting(name: &str, volumes: &str) -> Vec<String> {
    problems(&lab(&format!(
        "machines:\n  {name}: {{ networks: {{ lan: 10 }}, docker: {{ image: x }}, volumes: {volumes} }}\n"
    )))
}

/// The problems of shared volume `share` (written `volume`) mounted at /share by `a` and `b`
/// on `lan`, with the lab's other networks and machines in `more`.
fn shared(volume: &str, more: &str) -> Vec<String> {
    problems(&lab(&format!(
        "volumes:\n  share: {volume}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, vm: {{ os: debian@12 }}, volumes: {{ share: /share }} }}\n  b: {{ networks: {{ lan: 11 }}, vm: {{ os: debian@12 }}, volumes: {{ share: /share }} }}\n{more}"
    )))
}

/// The problems of `b` taking folder `sub` of the shared volume `a` holds.
fn folder(sub: &str) -> Vec<String> {
    shared(
        "{}",
        &format!("  c: {{ networks: {{ lan: 12 }}, vm: {{ os: debian@12 }}, volumes: {{ share: {{ path: /www, subpath: '{sub}' }} }} }}\n"),
    )
}

#[test]
fn a_volume_name_fits_its_kubernetes_claim_and_seed_container() {
    assert_eq!(mounting("web", &format!("{{ {}: /data }}", "v".repeat(58))), Vec::<String>::new());
    assert_eq!(
        mounting("web", &format!("{{ {}: /data }}", "v".repeat(59))),
        [format!(
            "machines.web.volumes.{}: volume names are at most 58 characters: the Kubernetes init container seeding it is `seed-<volume>`",
            "v".repeat(59)
        )]
    );
    assert_eq!(
        mounting("abcdefghij", &format!("{{ {}: /data }}", "v".repeat(53))),
        [format!(
            "machines.abcdefghij.volumes.{}: machine name and volume name together are at most 62 characters: the Kubernetes claim is `<machine>-<volume>`",
            "v".repeat(53)
        )]
    );
    assert_eq!(
        mounting("web", "{ Data: /data }"),
        ["machines.web.volumes.Data: volume names are kebab-case: lowercase letters, digits and dashes"]
    );
}

#[test]
fn a_volume_mounts_at_a_safe_absolute_path_once() {
    assert_eq!(mounting("web", "{ a: /var/lib/data, b: /a/..b/c }"), Vec::<String>::new());
    for wrong in ["/", "data", "/a/../b", "/a:b", "/a b"] {
        assert_eq!(
            mounting("web", &format!("{{ a: '{wrong}' }}")),
            [format!(
                "machines.web.volumes.a: `{wrong}` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"
            )],
            "{wrong}"
        );
    }
    assert_eq!(
        mounting("web", "{ a: /data, b: /data/ }"),
        ["machines.web.volumes.b: `/data/` is already a volume of this machine"]
    );
}

#[test]
fn a_shared_volume_is_reached_over_a_network_its_holder_is_on() {
    assert_eq!(shared("{}", ""), Vec::<String>::new());
    let found = problems(&lab(
        "  far: { cidr: 10.1.0.0/24 }\nvolumes:\n  share: {}\nmachines:\n  a: { networks: { lan: 10 }, vm: { os: debian@12 }, volumes: { share: /share } }\n  b: { networks: { far: 10 }, vm: { os: debian@12 }, volumes: { share: /share } }\n",
    ));
    assert_eq!(
        found,
        ["machines.b.volumes.share: `b` shares no network with `a`, which holds the volume (NFS on the VM targets)"]
    );
}

#[test]
fn a_shared_volumes_owner_mounts_it() {
    assert_eq!(shared("{ owner: b }", ""), Vec::<String>::new());
    assert_eq!(
        shared("{ owner: c }", ""),
        ["volumes.share.owner: `c` doesn't mount it: the owner is one of its machines (a, b)"]
    );
}

#[test]
fn a_windows_machine_mounts_no_shared_volume() {
    let windows = "  w: { networks: { lan: 12 }, vm: { os: windows-server@2022 }, volumes: { share: /share } }\n";
    assert_eq!(
        shared("{}", windows),
        ["machines.w.volumes.share: a shared volume is mounted over NFS: Linux machines only"]
    );
    assert_eq!(
        shared("{ virtiofs: {} }", windows),
        [
            "machines.w.volumes.share: a shared volume is mounted over NFS, or virtiofs on Proxmox, whose Windows driver (virtio-fs and WinFsp) the set-up doesn't install: Linux machines only"
        ]
    );
}

#[test]
fn a_host_folder_is_a_proxmox_mapping_id() {
    assert_eq!(shared("{ virtiofs: { mapping: data_1-a } }", ""), Vec::<String>::new());
    assert_eq!(
        shared("{ virtiofs: { mapping: 1data } }", ""),
        ["volumes.share.virtiofs.mapping: `1data` isn't a directory mapping id: a letter, then letters, digits, '-' and '_', at most 64"]
    );
}

#[test]
fn an_existing_volume_is_never_removed_and_named_as_docker_names_volumes() {
    let external = |name: &str| shared(&format!("{{ external: '{name}' }}"), "");
    assert_eq!(external("1a.b_c-d"), Vec::<String>::new());
    assert_eq!(external(&"a".repeat(63)), Vec::<String>::new());
    for wrong in ["a".repeat(64), "-a".to_owned(), "a b".to_owned()] {
        assert_eq!(
            external(&wrong),
            [format!(
                "volumes.share.external: `{wrong}` isn't a volume name: letters, digits, '-', '_' and '.', starting with a letter or digit, at most 63"
            )],
            "{wrong}"
        );
    }
    assert_eq!(
        shared("{ external: data, persistent: true }", ""),
        ["volumes.share.persistent: an external volume is never removed: leave `persistent` out"]
    );
}

#[test]
fn a_folder_is_a_relative_path_in_a_shared_volume() {
    assert_eq!(folder("www"), Vec::<String>::new());
    assert_eq!(folder("logs/app.1"), Vec::<String>::new());
    for wrong in ["", "/www", "a//b", "./a", "a/..", "a b"] {
        assert_eq!(
            folder(wrong),
            [format!(
                "machines.c.volumes.share.subpath: `{wrong}` isn't a folder inside the volume: a relative path like `www` or `logs/app`, no `..`"
            )],
            "{wrong:?}"
        );
    }
}

#[test]
fn only_a_shared_volume_has_folders_and_only_for_its_other_machines() {
    assert_eq!(
        mounting("web", "{ data: { path: /data, subpath: www } }"),
        ["machines.web.volumes.data.subpath: only a shared volume has folders to take: declare `data` under the top-level `volumes`"]
    );
    assert_eq!(
        shared(
            "{ owner: b }",
            "  c: { networks: { lan: 12 }, vm: { os: debian@12 }, volumes: { share: { path: /www, subpath: www } } }\n"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        shared(
            "{ owner: c }",
            "  c: { networks: { lan: 12 }, vm: { os: debian@12 }, volumes: { share: { path: /www, subpath: www } } }\n"
        ),
        ["machines.c.volumes.share.subpath: `c` holds the whole volume `share`: give the subpath to another machine, or make another one the `owner`"]
    );
}
