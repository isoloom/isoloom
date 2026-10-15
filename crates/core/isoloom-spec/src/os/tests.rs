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
        assert_eq!(resolve(text).ok().map(|o| o.name), Some(name), "{text}");
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
fn an_image_table_name_is_its_os_once_parsed_never_a_spelling() {
    for name in KNOWN_OS {
        let os = by_name(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(os.name, *name);
        assert_eq!(resolve(&os.id()).ok(), Some(os), "{name} is {}", os.id());
    }
    for name in ["debian-12", "windows-server-2019", "almalinux-9", "ubuntu-24.04"] {
        assert!(resolve(name).is_err(), "{name}");
    }
    assert_eq!(by_name("debian@12"), None);
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
    for s in ["debian", "debian@12", "kali", "kali@rolling", "windows-server@2022"] {
        assert_eq!(all.iter().filter(|x| *x == s).count(), 1, "{s}");
    }
    assert!(all.iter().all(|s| resolve(s).is_ok()));
    assert!(!all.iter().any(|s| s == "debian-12"), "image-table names are not spellings");
}
