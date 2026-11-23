//! What the refresh fetches to pin an entry, from its definition alone: each repository with its
//! key, the packages pinned to the release, the official image's tag, on each OS and release.

use isoloom_catalogue::{Family, Fetches, KeyCheck};

use crate::testing::demo;

/// What the demo entry's release `version` fetches on `os`.
fn fetches(version: &str, os: &str) -> Result<Fetches, Box<dyn std::error::Error>> {
    let catalogue = demo()?;
    let def = catalogue.entry("demo").ok_or("the demo entry")?.definition();
    let release = def.release(&version.parse()?).ok_or("the release")?;
    Ok(def.fetches(release, os.parse()?)?)
}

#[test]
fn an_apt_repository_comes_with_its_suite_its_components_and_its_keys_fingerprint() {
    let f = fetches("2.4.1", "debian@12").unwrap();
    assert_eq!(f.image, None, "debian@12 has no official image");
    assert_eq!(f.packages.iter().map(ToString::to_string).collect::<Vec<_>>(), ["demo"]);
    let r = &f.repositories[0];
    assert_eq!(
        (r.url.as_str(), r.family, r.suite.as_deref()),
        ("https://example.org/stable/debian", Family::Debian, Some("bookworm"))
    );
    assert_eq!(r.components, ["main"]);
    assert_eq!(
        (r.key_url.as_str(), &r.key),
        (
            "https://example.org/key.asc",
            &KeyCheck::Fingerprint("0123456789ABCDEF0123456789ABCDEF01234567".into())
        )
    );
}

#[test]
fn an_rpm_repository_has_no_suite_and_its_key_pinned_by_sha256() {
    let f = fetches("2.4.1", "rocky@9").unwrap();
    assert_eq!(f.repositories.len(), 1, "the Debian repository is the Debian family's only");
    let r = &f.repositories[0];
    assert_eq!(
        (r.url.as_str(), r.family, r.suite.as_deref()),
        ("https://example.org/stable/rpm/9", Family::RedHat, None)
    );
    assert_eq!(r.components, Vec::<String>::new());
    assert_eq!(r.key, KeyCheck::Sha256("1".repeat(64).parse().unwrap()));
}

#[test]
fn the_official_image_is_fetched_by_its_tag_on_its_os() {
    assert_eq!(fetches("2.4.1", "debian@13").unwrap().image.as_deref(), Some("docker.io/library/demo:2.4.1"));
}

#[test]
fn a_release_its_steps_dont_apply_to_fetches_nothing_of_theirs() {
    let edge = fetches("2.5.0", "debian@12").unwrap();
    assert_eq!(edge.repositories[0].url, "https://example.org/edge/debian");
    let distro = fetches("2.4", "ubuntu@24.04").unwrap();
    assert!(distro.repositories.is_empty(), "no repository step applies to Ubuntu");
    assert!(distro.packages.is_empty(), "a distro release pins no package");
}
