use isoloom_test_support::assert_every;

use super::*;

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
    for name in all().map(|os| os.name) {
        let os = by_name(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(os.name, name);
        assert_eq!(resolve(&os.id()).ok(), Some(os), "{name} is {}", os.id());
    }
    for name in ["debian-12", "windows-server-2019", "almalinux-9", "ubuntu-24.04"] {
        let e = resolve(name).unwrap_err().to_string();
        assert!(e.starts_with(&format!("unknown OS `{name}`")), "{name}: {e}");
    }
    assert_eq!(by_name("debian@12"), None);
}

#[test]
fn every_version_is_an_image_table_name() {
    let names: Vec<&str> = all().map(|os| os.name).collect();
    let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), names.len(), "each release has its own image-table name: {names:?}");
    for f in DISTROS {
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
}

#[test]
fn spellings_cover_every_form_once() {
    let all = spellings();
    for s in ["debian", "debian@12", "kali", "kali@rolling", "windows-server@2022"] {
        assert_eq!(all.iter().filter(|x| *x == s).count(), 1, "{s}");
    }
    assert_every!(&all, |s: &&String| resolve(s).is_ok());
    assert!(!all.iter().any(|s| s == "debian-12"), "image-table names are not spellings");
}

#[test]
fn every_linux_os_has_a_pinned_image_and_windows_none() {
    for os in all() {
        match os.container_image() {
            Some(image) => assert!(image.contains("@sha256:") && image.len() > 71, "{}: {image}", os.name),
            None => assert!(os.kind.is_windows(), "{} has no image", os.name),
        }
    }
}

#[test]
fn each_os_gets_its_own_image() {
    let name = |os| {
        by_name(os)
            .and_then(|o| o.container_image())
            .and_then(|image| image.split_once('@'))
            .map(|(name, _)| name)
    };
    assert_eq!(name("alpine-3.19"), Some("alpine:3.19"));
    assert_eq!(name("kali"), Some("kalilinux/kali-rolling"));
    assert_eq!(name("windows-11"), None);
}

#[test]
fn the_kind_is_the_distributions_and_only_apt_releases_have_a_codename() {
    for d in DISTROS {
        assert_eq!(d.kind == Kind::Windows, d.installer == Installer::Windows, "{}", d.name);
        for r in d.releases {
            assert_eq!(r.codename.is_some(), d.installer == Installer::Apt, "{}", r.name);
            assert_eq!(Kind::of(r.name), d.kind, "{}", r.name);
        }
        assert_eq!(Kind::of(d.name), d.kind, "{}", d.name);
        // As a spec writes it, with a version.
        assert_eq!(Kind::of(&format!("{}@{}", d.name, d.default)), d.kind, "{}", d.name);
    }
    assert_eq!(by_name("debian-12").and_then(|o| o.codename()), Some("bookworm"));
    let tpm: Vec<&str> = all().filter(Os::expects_tpm).map(|o| o.name).collect();
    assert_eq!(tpm, ["windows-11"]);
}
