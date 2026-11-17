//! The catalogue built into Isoloom: it loads, keeps to its budget, and resolves nginx on every
//! OS its default release claims, with the official image only where its OS is the machine's.

use isoloom_catalogue::Catalogue;
use isoloom_spec::model::Arch;
use isoloom_spec::syntax::{FileState, Step};

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
fn nginx_with_a_site_serves_it_in_place_of_its_own_page_and_passes_php_to_the_fpm_machine() {
    let spec = isoloom_spec::syntax::parse(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, service: nginx, with: { root: site/, php_fpm: 'fpm:9000' }, networks: { lab: 10 } }\n  fpm: { os: debian@12, networks: { lab: 20 } }\n",
    )
    .unwrap();
    let r = embedded().unwrap().resolve(&spec, "web").unwrap().remove(0);
    let kinds: Vec<_> = r.configure.iter().map(Step::kind).collect();
    assert_eq!(kinds, ["template", "file", "template", "file", "copy", "firewall", "service"]);
    let [Step::Template(_), _, Step::Template(php), Step::File(page), Step::Copy(site), ..] = r.configure.as_slice() else {
        panic!("{:?}", r.configure)
    };
    assert_eq!(
        (page.path.as_str(), &page.state),
        ("/usr/share/nginx/html", &FileState::Absent),
        "nginx's own page goes first"
    );
    assert_eq!((site.from.as_str(), site.to.as_str()), ("site/", "/usr/share/nginx/html/"));
    assert_eq!(php.to.as_str(), "/etc/nginx/isoloom.d/php.conf");
    let php_conf = r.files.get(&php.from).map(String::as_str).unwrap_or_default();
    let pass: Vec<_> = php_conf.lines().filter(|l| l.trim_start().starts_with("fastcgi_pass")).collect();
    assert_eq!(
        pass,
        ["    fastcgi_pass  {{ machines.fpm.addresses.lab }}:9000;"],
        "the fpm machine's address, per target"
    );
    assert!(r.start.is_empty(), "{:?}", r.start);
}

#[test]
fn php_installs_the_server_its_setting_is() {
    let run = |with: &str| {
        let r = resolve(embedded().unwrap(), "debian@12", "php@7.4", with).unwrap();
        r.configure
            .iter()
            .filter_map(|s| if let Step::Service(s) = s { Some(s.name().to_string()) } else { None })
            .collect::<Vec<_>>()
    };
    assert_eq!(run("{ server: apache }"), ["apache2"]);
    assert_eq!(run("{ server: fpm }"), ["php-fpm"]);
    assert_eq!(run("{}"), Vec::<String>::new(), "the CLI alone");
}

/// The download steps of `steps`: each one's architectures and the archive each fetches.
fn downloads(steps: &[Step]) -> Vec<Vec<(Arch, String)>> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::File(f) => match &f.state {
                FileState::Downloads(all) => Some(
                    all.iter()
                        .map(|d| (d.arch, d.url.as_str().rsplit('/').next().unwrap_or_default().to_owned()))
                        .collect(),
                ),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// MySQL 8.4.7 installs Oracle's build of the machine's architecture on a VM (x86-64 or arm64,
/// on Ubuntu 22.04 and Debian 12 alike), and runs its official image, published for both, in a
/// container on Debian 12; the older releases have an x86-64 build only.
#[test]
fn mysql_8_4_7_installs_the_build_of_the_machines_architecture() {
    let with = "{ password: root-secret, user_password: app-secret, port: 3207, privileges: SELECT }";
    for os in ["ubuntu@22.04", "debian@12"] {
        let r = resolve(embedded().unwrap(), os, "mysql@8.4.7", with).unwrap_or_else(|p| panic!("{os}: {p:?}"));
        assert_eq!(
            downloads(&r.install),
            [vec![
                (Arch::Amd64, "mysql-8.4.7-linux-glibc2.17-x86_64.tar.xz".to_owned()),
                (Arch::Arm64, "mysql-8.4.7-linux-glibc2.28-aarch64.tar.xz".to_owned())
            ]],
            "{os}"
        );
        let packages: Vec<String> = r
            .install
            .iter()
            .filter_map(|s| if let Step::Packages(p) = s { Some(p) } else { None })
            .flatten()
            .map(|p| p.name.to_string())
            .collect();
        assert!(packages.iter().any(|p| p == "libncurses6"), "{os}: the arm64 client's ncurses: {packages:?}");
        assert!(matches!(r.configure.last(), Some(Step::Service(s)) if s.name().as_str() == "mysql"), "{os}");
        match (os, &r.image) {
            ("debian@12", Some(image)) => {
                assert!(image.reference.starts_with("docker.io/library/mysql:8.4.7@sha256:"), "{}", image.reference);
                assert_eq!(image.platforms, [Arch::Amd64, Arch::Arm64]);
            }
            ("ubuntu@22.04", None) => {}
            (_, image) => panic!("{os}: {image:?}"),
        }
    }
    let older = resolve(embedded().unwrap(), "ubuntu@22.04", "mysql@8.0.44", with).unwrap();
    assert_eq!(
        downloads(&older.install),
        [vec![(Arch::Amd64, "mysql-8.0.44-linux-glibc2.17-x86_64.tar.xz".to_owned())]]
    );
}
