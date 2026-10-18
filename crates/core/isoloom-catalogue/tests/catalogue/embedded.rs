//! The catalogue built into Isoloom: it loads, keeps to its budget, and resolves nginx on every
//! OS its default release claims, with the official image only where its OS is the machine's.

use isoloom_catalogue::Catalogue;
use isoloom_spec::syntax::Step;

use crate::testing::resolve;

fn embedded() -> Result<&'static Catalogue, &'static isoloom_catalogue::Error> {
    Catalogue::embedded()
}

#[test]
fn the_built_in_catalogue_loads_and_stays_under_its_budget() {
    assert!(embedded().unwrap().names().contains(&"nginx"), "{:?}", embedded().unwrap().names());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../catalogue");
    let size: u64 = walk(&root).unwrap();
    assert!(size < 1_000_000, "the catalogue is {size} bytes: it stays under 1 MB");
}

/// The size of every file under `dir`.
fn walk(dir: &std::path::Path) -> std::io::Result<u64> {
    let mut size = 0;
    for e in std::fs::read_dir(dir)? {
        let path = e?.path();
        size += if path.is_dir() { walk(&path)? } else { path.metadata()?.len() };
    }
    Ok(size)
}

#[test]
fn nginx_resolves_on_each_os_of_its_default_release() {
    for os in ["debian@12", "debian@13", "ubuntu@24.04", "rocky@9", "alpine@3.24"] {
        let r = resolve(embedded().unwrap(), os, "nginx", "{}").unwrap_or_else(|p| panic!("{os}: {p:?}"));
        let official = matches!(os, "debian@13" | "alpine@3.24");
        assert_eq!(r.image.is_some(), official, "{os}: the official image only where its OS is the machine's");
        assert!(matches!(r.install.last(), Some(Step::Packages(_))), "{os}");
        assert!(matches!(r.configure.last(), Some(Step::Service(_))), "{os}");
    }
}

#[test]
fn nginx_at_1_31_is_mainline_from_its_own_repository() {
    let r = resolve(embedded().unwrap(), "debian@12", "nginx@1.31", "{}").unwrap();
    assert_eq!(r.release.as_str(), "1.31.7");
    let Some(Step::Repository(repo)) = r.install.first() else {
        panic!("{:?}", r.install)
    };
    assert_eq!(repo.url.as_str(), "https://nginx.org/packages/mainline/debian");
}

#[test]
fn nginx_resolved_on_debian_12_with_a_site_and_php() {
    let spec = isoloom_spec::syntax::parse(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, service: nginx, with: { root: site/, php_fpm: 'fpm:9000' }, networks: { lab: 10 } }\n  fpm: { os: debian@12, networks: { lab: 20 } }\n",
    )
    .unwrap();
    let r = embedded().unwrap().resolve(&spec, "web").unwrap().remove(0);
    insta::assert_snapshot!(format!("{:#?}\n{:#?}\n{:#?}\n{:#?}", r.install, r.configure, r.start, r.files));
}
