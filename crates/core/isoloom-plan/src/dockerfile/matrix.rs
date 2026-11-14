//! The base images a translated Dockerfile may start `FROM` without fetching anything, and what
//! each is on a fresh VM. A distro image is its OS (`debian:bookworm-slim` is a `debian@12` VM). An
//! official service image is a catalogue entry ([`EntryCall`]: `mysql:5.7` is the catalogue's
//! `mysql`, its newest 5.7 release), resolved by the translation with the settings the image's
//! environment gives (`MYSQL_ROOT_PASSWORD` and the like); the images whose entries have no VM
//! recipe yet are still the install written here. Either wins over a fetched Dockerfile.
//!
//! An entry's release goes on the machine's own `os` when the release runs there, else on the
//! first OS it lists.

use super::instruction::Command;

/// What a base image is on a VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    /// The OS the VM installs (`debian@12`).
    pub os: String,
    /// Shell run first, as root: what the image's own build installed.
    pub prelude: Vec<String>,
    /// The image's environment.
    pub env: Vec<(String, String)>,
    /// The image's working folder.
    pub workdir: Option<String>,
    /// The image's user.
    pub user: Option<String>,
    /// The image's entrypoint.
    pub entrypoint: Option<Command>,
    /// The image's command.
    pub cmd: Option<Command>,
    /// The catalogue entry the image is, which the translation resolves (its OS then the
    /// release's): `None` for a distro or an install written here.
    pub entry: Option<EntryCall>,
    /// kCTF's challenge base: it takes the VM's OS (its chroot stage's), and the VM is the
    /// jail, so copying a stage's whole filesystem into it is nothing to do.
    pub jail: bool,
}

/// A catalogue entry a base image is: the entry, the version its tag asks for (`None`: the
/// newest), and what a required setting the image's environment leaves out takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryCall {
    /// The entry (`mysql`).
    pub entry: &'static str,
    /// The version the tag names (`5.7`), or none for the newest release with a VM form.
    pub version: Option<String>,
    /// A setting the image's environment leaves out, and the setting whose value it takes or the
    /// value itself.
    pub unset: &'static [(&'static str, Fallback)],
    /// Prefixes the image reads its environment under besides the entry's own: `MARIADB_` and
    /// `MYSQL_` (a mariadb image reads `MYSQL_ROOT_PASSWORD` too).
    pub aliases: &'static [(&'static str, &'static str)],
}

/// What a setting the image's environment leaves out takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// This value.
    Value(&'static str),
    /// The value of this other setting.
    Setting(&'static str),
}

/// MySQL's image creates no application user when `MYSQL_USER` is unset; the entry's
/// entrypoint reads the user `root` the same way, so the image's behaviour is kept.
const MYSQL_UNSET: &[(&str, Fallback)] = &[("user", Fallback::Value("root")), ("user_password", Fallback::Setting("password"))];

/// The php image's variants: the entry's `server`.
const PHP_APACHE: &[(&str, Fallback)] = &[("server", Fallback::Value("apache"))];
const PHP_FPM: &[(&str, Fallback)] = &[("server", Fallback::Value("fpm"))];

/// PostgreSQL's image makes its superuser `postgres` when `POSTGRES_USER` is unset, and names the
/// database after the user when `POSTGRES_DB` is.
const POSTGRES_UNSET: &[(&str, Fallback)] = &[("user", Fallback::Value("postgres")), ("database", Fallback::Setting("user"))];

impl Base {
    /// A bare OS: no prelude, root in `/`, a shell for a command.
    fn os(os: &str) -> Base {
        Base {
            os: os.to_owned(),
            prelude: Vec::new(),
            env: Vec::new(),
            workdir: None,
            user: None,
            entrypoint: None,
            cmd: None,
            entry: None,
            jail: false,
        }
    }

    /// The catalogue's `entry`, at the version `tag` names, with what a setting the image's
    /// environment leaves out takes and the prefixes it reads that environment under too.
    fn entry(entry: &'static str, tag: &str, unset: &'static [(&'static str, Fallback)], aliases: &'static [(&'static str, &'static str)]) -> Base {
        let (version, _) = version_and_variants(tag);
        let version = (!["latest", "lts", "current", "slim", "alpine"].contains(&version.as_str())).then_some(version);
        Base {
            entry: Some(EntryCall {
                entry,
                version,
                unset,
                aliases,
            }),
            ..Base::os("")
        }
    }
}

/// Why an image has no entry, or the entry can't take the OS asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoEntry {
    /// The matrix has no such image: a fetched Dockerfile may still give it.
    Unknown(String),
    /// The image can't be a VM, whatever is fetched.
    Refused {
        /// Why.
        reason: String,
        /// What to do instead.
        fix: String,
    },
}

/// An image reference split into its name (without `docker.io/library/`) and tag (`latest` when
/// it names none); a digest is dropped (the VM installs by version).
#[must_use]
pub fn split(image: &str) -> (String, String) {
    let image = image.split('@').next().unwrap_or(image);
    let image = ["docker.io/library/", "index.docker.io/library/", "docker.io/", "library/"]
        .iter()
        .find_map(|p| image.strip_prefix(p))
        .unwrap_or(image);
    let last_slash = image.rfind('/').map_or(0, |i| i + 1);
    match image.get(last_slash..).and_then(|tail| tail.rfind(':')) {
        Some(colon) => {
            let at = last_slash + colon;
            (image.get(..at).unwrap_or(image).to_owned(), image.get(at + 1..).unwrap_or_default().to_owned())
        }
        None => (image.to_owned(), "latest".to_owned()),
    }
}

/// The base image `image` as a VM.
///
/// # Errors
///
/// [`NoEntry`]: no entry (`Unknown`), or one that can't be a VM at all (`Refused`).
pub fn of(image: &str) -> Result<Base, NoEntry> {
    let (name, tag) = split(image);
    match name.as_str() {
        "debian" | "ubuntu" | "alpine" | "rockylinux" | "rockylinux/rockylinux" | "almalinux" | "fedora" | "centos" | "kalilinux/kali-rolling" => {
            distro(&name, &tag).map(|os| Base::os(&os))
        }
        "php" => Ok(php(&tag)),
        "mariadb" => Ok(Base::entry("mariadb", &tag, MYSQL_UNSET, &[("MARIADB_", "MYSQL_")])),
        "mysql" | "mysql/mysql-server" => Ok(Base::entry("mysql", &tag, MYSQL_UNSET, &[])),
        "postgres" => Ok(Base::entry("postgres", &tag, POSTGRES_UNSET, &[])),
        "python" => Ok(Base::entry("python", &tag, &[], &[])),
        "node" => Ok(Base::entry("node", &tag, &[], &[])),
        // The image's `USER` and the folders its entrypoints read: a lab's own entrypoint runs.
        "gitea/gitea" => Ok(Base {
            env: vec![("USER".to_owned(), "git".to_owned()), ("GITEA_CUSTOM".to_owned(), "/data/gitea".to_owned())],
            ..Base::entry("gitea", &tag, &[], &[])
        }),
        // The image's GOPATH, its PATH and its folder: what a Dockerfile's `go build` reads.
        "golang" => Ok(Base {
            env: vec![
                ("GOPATH".to_owned(), "/go".to_owned()),
                (
                    "PATH".to_owned(),
                    "/go/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_owned(),
                ),
            ],
            workdir: Some("/go".to_owned()),
            ..Base::entry("golang", &tag, &[], &[])
        }),
        // kCTF's challenge base: its OS is the VM's (an empty `os` takes it), its jail the VM.
        "gcr.io/kctf-docker/challenge" => Ok(Base {
            prelude: vec![KCTF.to_owned()],
            jail: true,
            ..Base::os("")
        }),
        "scratch" => Err(NoEntry::Refused {
            reason: "`FROM scratch` has no OS to install onto".to_owned(),
            fix: "give the machine `os` and `app.steps`: its VM form".to_owned(),
        }),
        _ => Err(NoEntry::Unknown(name)),
    }
}

/// The distro releases Isoloom has a VM image of: an image's name, the releases its tags name
/// (a version or a codename), and the OS.
const DISTROS: &[(&str, &[&str], &str)] = &[
    ("debian", &["11", "bullseye"], "debian@11"),
    ("debian", &["12", "bookworm", "latest", "stable"], "debian@12"),
    ("debian", &["13", "trixie"], "debian@13"),
    ("ubuntu", &["14.04", "trusty"], "ubuntu@14.04"),
    ("ubuntu", &["16.04", "xenial"], "ubuntu@16.04"),
    ("ubuntu", &["18.04", "bionic"], "ubuntu@18.04"),
    ("ubuntu", &["20.04", "focal"], "ubuntu@20.04"),
    ("ubuntu", &["22.04", "jammy"], "ubuntu@22.04"),
    ("ubuntu", &["24.04", "noble", "latest"], "ubuntu@24.04"),
    ("alpine", &["3.7"], "alpine@3.7"),
    ("alpine", &["3.19"], "alpine@3.19"),
    ("alpine", &["3.24", "latest"], "alpine@3.24"),
    ("rockylinux", &["9"], "rocky@9"),
    ("rockylinux/rockylinux", &["9"], "rocky@9"),
    ("almalinux", &["9"], "almalinux@9"),
    ("fedora", &["42"], "fedora@42"),
    ("centos", &["7", "centos7"], "centos@7"),
    ("kalilinux/kali-rolling", &["latest", "rolling"], "kali@rolling"),
];

/// The OS of distro image `name:tag`.
fn distro(name: &str, tag: &str) -> Result<String, NoEntry> {
    // `bookworm-20250929-slim`, `12-slim`, `3.19.1`, `9.3`: the release, without the build's
    // suffixes; Alpine and the RHEL family by their first one or two numbers.
    let release = tag.split('-').next().unwrap_or(tag);
    let short = match name {
        "alpine" => release.split('.').take(2).collect::<Vec<_>>().join("."),
        "rockylinux" | "rockylinux/rockylinux" | "almalinux" => release.split('.').next().unwrap_or(release).to_owned(),
        _ => release.to_owned(),
    };
    DISTROS
        .iter()
        .find(|(n, releases, _)| *n == name && releases.contains(&short.as_str()))
        .map(|(_, _, os)| (*os).to_owned())
        .ok_or_else(|| NoEntry::Refused {
            reason: format!("Isoloom has no VM image of `{name}:{tag}`"),
            fix: "build FROM a release Isoloom has a VM image of, or give the machine `os` and `app.steps`".to_owned(),
        })
}

/// A tag's leading version (`8.5-apache` → `8.5`), and its variant words (`apache`).
fn version_and_variants(tag: &str) -> (String, Vec<String>) {
    let mut words = tag.split('-');
    let version = words.next().unwrap_or_default().to_owned();
    (version, words.map(str::to_owned).collect())
}

const KCTF: &str = include_str!("matrix/kctf.sh");

/// `php:<version>[-apache|-fpm]`: the catalogue's php at that version, its `server` the variant
/// (none for the CLI image), with the image's `PHP_INI_DIR` and folder.
fn php(tag: &str) -> Base {
    let (_, variants) = version_and_variants(tag);
    let unset = if variants.iter().any(|v| v == "apache") {
        PHP_APACHE
    } else if variants.iter().any(|v| v == "fpm") {
        PHP_FPM
    } else {
        &[]
    };
    Base {
        env: vec![("PHP_INI_DIR".into(), "/usr/local/etc/php".into())],
        workdir: Some("/var/www/html".into()),
        ..Base::entry("php", tag, unset, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_split_into_name_and_tag() {
        assert_eq!(split("docker.io/library/php:8.5-apache"), ("php".into(), "8.5-apache".into()));
        assert_eq!(split("debian@sha256:abc"), ("debian".into(), "latest".into()));
        assert_eq!(split("localhost:5000/team/app:1"), ("localhost:5000/team/app".into(), "1".into()));
    }

    #[test]
    fn distro_images_are_their_os() {
        for (image, os) in [
            ("debian:bookworm-20250929-slim", "debian@12"),
            ("debian:bullseye-slim", "debian@11"),
            ("ubuntu:22.04", "ubuntu@22.04"),
            ("alpine:3.19.1", "alpine@3.19"),
            ("kalilinux/kali-rolling", "kali@rolling"),
        ] {
            assert_eq!(of(image).unwrap().os, os, "{image}");
        }
        assert_eq!(of("alpine:3.7").unwrap().os, "alpine@3.7");
        assert!(matches!(of("alpine:3.6"), Err(NoEntry::Refused { reason, .. }) if reason.contains("`alpine:3.6`")));
    }

    #[test]
    fn service_images_pin_their_version() {
        let php = of("php:7.4-apache").unwrap();
        let call = php.entry.expect("the catalogue's php");
        assert_eq!((call.entry, call.version.as_deref()), ("php", Some("7.4")));
        assert_eq!(call.unset, PHP_APACHE);
        assert_eq!(of("php:8.2-fpm").unwrap().entry.map(|c| c.unset), Some(PHP_FPM));
        assert_eq!(of("php:8.2-cli").unwrap().entry.map(|c| c.unset.len()), Some(0));
        assert_eq!(php.workdir.as_deref(), Some("/var/www/html"));
    }

    #[test]
    fn catalogue_images_name_their_entry_and_the_version_their_tag_asks_for() {
        let call = |image: &str| of(image).unwrap().entry.map(|c| (c.entry, c.version));
        assert_eq!(call("mysql:5.7"), Some(("mysql", Some("5.7".to_owned()))));
        assert_eq!(call("docker.io/library/mysql:5.7.44"), Some(("mysql", Some("5.7.44".to_owned()))));
        assert_eq!(call("mysql/mysql-server:8.0.21"), Some(("mysql", Some("8.0.21".to_owned()))));
        assert_eq!(call("python:3.8-slim-buster"), Some(("python", Some("3.8".to_owned()))));
        assert_eq!(call("node:lts"), Some(("node", None)));
        assert_eq!(call("docker.io/library/mariadb:10.6.3"), Some(("mariadb", Some("10.6.3".to_owned()))));
        assert_eq!(call("golang:1.23-alpine"), Some(("golang", Some("1.23".to_owned()))));
        assert_eq!(call("gitea/gitea:1.22@sha256:538658de"), Some(("gitea", Some("1.22".to_owned()))));
        assert_eq!(of("golang:1.22-bookworm").unwrap().workdir.as_deref(), Some("/go"));
        assert_eq!(call("postgres:15.4-alpine"), Some(("postgres", Some("15.4".to_owned()))));
        assert_eq!(call("debian:12"), None);
    }

    #[test]
    fn an_image_without_an_entry_is_unknown_and_scratch_is_refused() {
        assert_eq!(of("vulhub/tomcat:8.5"), Err(NoEntry::Unknown("vulhub/tomcat".into())));
        assert!(matches!(of("scratch"), Err(NoEntry::Refused { .. })));
    }
}
