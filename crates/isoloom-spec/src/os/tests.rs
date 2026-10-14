use super::*;
use crate::model::KNOWN_OS;

#[test]
fn a_family_and_version_map_onto_the_image_tables_name() {
    for (text, name) in [
        ("debian@12", "debian-12"),
        ("ubuntu@22.04", "ubuntu-22.04"),
        ("alma@9", "almalinux-9"),
        ("kali@rolling", "kali"),
        ("windows@11", "windows-11"),
        ("windows-server@2022", "windows-server-2022"),
        ("windows-server@2008r2", "windows-server-2008r2"),
        ("alpine@3.19", "alpine-3.19"),
    ] {
        assert_eq!(resolve(text).map(|o| o.name), Ok(name), "{text}");
    }
}

#[test]
fn a_family_alone_is_its_pinned_default() {
    let defaults: Vec<String> = ["debian", "ubuntu", "rocky", "alma", "alpine", "kali", "fedora", "windows", "windows-server"]
        .iter()
        .map(|f| resolve(f).map(|o| o.id()).unwrap_or_default())
        .collect();
    assert_eq!(
        defaults,
        [
            "debian@12",
            "ubuntu@24.04",
            "rocky@9",
            "alma@9",
            "alpine@3.19",
            "kali@rolling",
            "fedora@42",
            "windows@11",
            "windows-server@2022"
        ]
    );
}

#[test]
fn the_older_names_are_aliases() {
    for name in KNOWN_OS {
        let os = resolve(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(os.name, *name);
        assert_eq!(resolve(&os.id()), Ok(os), "{name} and {} are the same OS", os.id());
    }
}

#[test]
fn every_version_is_an_image_table_name() {
    let names: Vec<&str> = FAMILIES.iter().flat_map(|f| f.versions.iter().map(|(_, n)| *n)).collect();
    assert_eq!(names, KNOWN_OS, "the families cover the image table, in its order");
    for f in FAMILIES {
        assert!(f.get(f.default).is_some(), "{}'s default {} is one of its versions", f.name, f.default);
    }
}

#[test]
fn unknown_families_and_versions_list_the_known_ones() {
    let family = resolve("gentoo@1").unwrap_err().to_string();
    assert!(
        family.starts_with("unknown OS `gentoo@1`; write `<family>@<version>` with one of: debian (11, 12, 13), ubuntu ("),
        "{family}"
    );
    assert!(family.contains("windows-server (2008r2, 2012r2, 2016, 2019, 2022, 2025)"), "{family}");
    let version = resolve("debian@10").unwrap_err().to_string();
    assert_eq!(version, "debian has no version `10`; use one of: 11, 12, 13 (`debian` alone is debian@12)");
    assert!(resolve("debian-10").is_err());
}

#[test]
fn spellings_cover_every_form_once() {
    let all = spellings();
    for s in [
        "debian",
        "debian@12",
        "debian-12",
        "kali",
        "kali@rolling",
        "windows-server@2022",
        "windows-server-2022",
    ] {
        assert_eq!(all.iter().filter(|x| *x == s).count(), 1, "{s}");
    }
    assert!(all.iter().all(|s| resolve(s).is_ok()));
}

#[test]
fn the_machines_os_reaches_its_vm_before_the_groups() {
    let mut doc: Value = serde_yaml_ng::from_str(
        "machines:\n  a: { os: debian@12 }\n  b: { os: windows, vm: { provision: [x.ps1] } }\n  c: { os: debian, vm: { os: debian-13 } }\n  d: { os: debian, docker: { image: x } }\n",
    )
    .unwrap();
    fill_vm(&mut doc);
    let vm = |m: &str| serde_yaml_ng::to_string(&doc["machines"][m].get("vm")).unwrap();
    assert_eq!(vm("a"), "os: debian@12\n");
    assert_eq!(vm("b"), "provision:\n- x.ps1\nos: windows\n");
    assert_eq!(vm("c"), "os: debian-13\n", "its own vm.os stays, for validation to compare");
    assert_eq!(vm("d"), "null\n", "a container keeps its `docker:` alone");
}
