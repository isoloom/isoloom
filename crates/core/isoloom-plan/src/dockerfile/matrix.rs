//! The service matrix: the base images a translated Dockerfile may start `FROM` without fetching
//! anything, and what each is on a fresh VM. A distro image is its OS (`debian:bookworm-slim` is a
//! `debian@12` VM). A classic service's official image (php, mariadb, mysql, postgres, python,
//! node) is the install that gives the same service at the same version, the conventions its
//! Dockerfile sets up (its helper commands, its entrypoint's first start driven by
//! `MYSQL_ROOT_PASSWORD` and the like), and its environment, folder and command. The matrix wins
//! over a fetched Dockerfile: it is shorter and kept here.
//!
//! An entry installs on the Debian family by default, on the machine's own `os` when the spec
//! names one the entry supports (`os: rocky@9` beside `app.build` puts MariaDB on Rocky Linux).

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
}

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

/// The OS families an entry installs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Families {
    /// apt only.
    Debian,
    /// apt or dnf.
    DebianOrRhel,
    /// Any Linux with glibc (a vendor's build unpacked).
    AnyGlibc,
}

impl Families {
    /// `wanted` when the entry installs there, else its default (`default`).
    fn pick(self, wanted: Option<&str>, default: &str) -> String {
        let family = |os: &str| os.split('@').next().unwrap_or(os).to_owned();
        let ok = match wanted.map(family).as_deref() {
            Some("debian" | "ubuntu") => true,
            Some("rocky" | "almalinux" | "fedora" | "centos") => self != Families::Debian,
            Some("kali") => self != Families::DebianOrRhel,
            _ => false,
        };
        match wanted {
            Some(os) if ok => os.to_owned(),
            _ => default.to_owned(),
        }
    }
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

/// The base image `image` as a VM, on `wanted` (the machine's own `os`) when the entry installs
/// there.
///
/// # Errors
///
/// [`NoEntry`]: no entry (`Unknown`), or one that can't be a VM at all (`Refused`).
pub fn of(image: &str, wanted: Option<&str>) -> Result<Base, NoEntry> {
    let (name, tag) = split(image);
    match name.as_str() {
        "debian" | "ubuntu" | "alpine" | "rockylinux" | "rockylinux/rockylinux" | "almalinux" | "fedora" | "centos" | "kalilinux/kali-rolling" => {
            distro(&name, &tag).map(|os| Base::os(&os))
        }
        "php" => Ok(php(&tag)),
        "mariadb" => Ok(mariadb(&tag, wanted)),
        "mysql" => mysql(&tag, wanted),
        "postgres" => Ok(postgres(&tag, wanted)),
        "python" => Ok(python(&tag, wanted)),
        "node" => Ok(node(&tag, wanted)),
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

/// `major.minor` of a version (`8.5.1` → `8.5`); a bare major stays as it is.
fn minor(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

const PHP: &str = include_str!("matrix/php.sh");
const PHP_SHIMS: &str = include_str!("matrix/php-shims.sh");
const MARIADB: &str = include_str!("matrix/mariadb.sh");
const MYSQL: &str = include_str!("matrix/mysql.sh");
const DB_ENTRYPOINT: &str = include_str!("matrix/db-entrypoint.sh");
const POSTGRES: &str = include_str!("matrix/postgres.sh");
const PYTHON: &str = include_str!("matrix/python.sh");
const NODE: &str = include_str!("matrix/node.sh");

/// `php:<version>[-apache|-fpm|-cli]`: PHP of that version from the Debian PHP archive
/// (`packages.sury.org`), Apache with `mod_php` for `-apache`, FPM on port 9000 for `-fpm`, and the
/// image's helpers (`docker-php-ext-install` installs the extension's package,
/// `apache2-foreground`).
fn php(tag: &str) -> Base {
    let (version, variants) = version_and_variants(tag);
    let version = match minor(&version).as_str() {
        "5" => "5.6".to_owned(),
        "7" => "7.4".to_owned(),
        "8" | "latest" => "8.4".to_owned(),
        v => v.to_owned(),
    };
    let apache = variants.iter().any(|v| v == "apache");
    let fpm = variants.iter().any(|v| v == "fpm");
    let sapi = if apache {
        "apache2"
    } else if fpm {
        "fpm"
    } else {
        "cli"
    };
    let fill = |s: &str| s.replace("{VERSION}", &version).replace("{SAPI}", sapi);
    // The CLI image's command is PHP's interactive shell: on a VM, nothing runs.
    let cmd = if apache {
        Some(exec(&["apache2-foreground"]))
    } else if fpm {
        Some(exec(&["php-fpm"]))
    } else {
        None
    };
    Base {
        os: "debian@12".to_owned(),
        prelude: vec![fill(PHP), fill(PHP_SHIMS)],
        env: vec![("PHP_INI_DIR".into(), "/usr/local/etc/php".into())],
        workdir: Some("/var/www/html".into()),
        user: None,
        entrypoint: None,
        cmd,
    }
}

/// `mariadb:<version>`: MariaDB of that version (Debian 12's own 10.11, else MariaDB's
/// repository), on every address, with the image's entrypoint.
fn mariadb(tag: &str, wanted: Option<&str>) -> Base {
    let (version, _) = version_and_variants(tag);
    let version = if version == "latest" { "11.8".to_owned() } else { minor(&version) };
    database(
        Families::DebianOrRhel.pick(wanted, "debian@12"),
        MARIADB.replace("{VERSION}", &version),
        "mariadbd",
    )
}

/// The full release `mysql:<version>` names, and Oracle's archive of it. A tag names a minor
/// (`5.7`) or a release (`5.7.44`); a minor is its last release.
fn mysql_release(version: &str) -> Option<(String, String)> {
    let full = match version {
        _ if version.matches('.').count() == 2 => version,
        "5" | "5.7" => "5.7.44",
        "5.5" => "5.5.62",
        "5.6" => "5.6.51",
        "8" | "8.0" => "8.0.44",
        "8.4" | "latest" => "8.4.7",
        _ => return None,
    }
    .to_owned();
    let series = minor(&full);
    let url = match series.as_str() {
        "5.5" | "5.6" | "5.7" => format!("https://cdn.mysql.com/archives/mysql-{series}/mysql-{full}-linux-glibc2.12-x86_64.tar.gz"),
        _ => format!("https://cdn.mysql.com/archives/mysql-{series}/mysql-{full}-linux-glibc2.17-x86_64.tar.xz"),
    };
    Some((full, url))
}

/// `mysql:<version>`: Oracle's generic Linux build of that release, with the image's entrypoint.
/// 5.x runs on Debian 11 (the last with the ncurses 5 its tools link).
fn mysql(tag: &str, wanted: Option<&str>) -> Result<Base, NoEntry> {
    let (version, _) = version_and_variants(tag);
    let Some((full, url)) = mysql_release(&version) else {
        return Err(NoEntry::Unknown(format!("mysql:{tag}")));
    };
    let old = full.starts_with('5');
    let os = Families::AnyGlibc.pick(wanted, if old { "debian@11" } else { "debian@12" });
    let ncurses = if old { "libncurses5" } else { "libncurses6" };
    let install = MYSQL
        .replace("{FULL}", &full)
        .replace("{VERSION}", &version)
        .replace("{URL}", &url)
        .replace("{NCURSES}", ncurses);
    Ok(database(os, install, "mysqld"))
}

/// A MySQL-family server: its install, then the shared entrypoint, run as `server`.
fn database(os: String, install: String, server: &str) -> Base {
    Base {
        os,
        prelude: vec![install, DB_ENTRYPOINT.to_owned()],
        env: Vec::new(),
        workdir: None,
        user: None,
        entrypoint: Some(exec(&["docker-entrypoint.sh"])),
        cmd: Some(exec(&[server])),
    }
}

/// `postgres:<version>`: the PostgreSQL project's build of that major (apt.postgresql.org, or
/// its dnf repository), with the image's entrypoint (`POSTGRES_PASSWORD`, `POSTGRES_USER`,
/// `POSTGRES_DB`, `/docker-entrypoint-initdb.d`).
fn postgres(tag: &str, wanted: Option<&str>) -> Base {
    let (version, _) = version_and_variants(tag);
    let major = if version == "latest" {
        "18".to_owned()
    } else {
        // 9.6 names its major with the minor; 10 and later by the first number.
        let parts: Vec<&str> = version.split('.').collect();
        match parts.as_slice() {
            ["9", minor, ..] => format!("9.{minor}"),
            [major, ..] => (*major).to_owned(),
            [] => version.clone(),
        }
    };
    Base {
        os: Families::DebianOrRhel.pick(wanted, "debian@12"),
        prelude: vec![POSTGRES.replace("{VERSION}", &major)],
        env: vec![("PGDATA".into(), "/var/lib/postgresql/data".into())],
        workdir: None,
        user: None,
        entrypoint: Some(exec(&["docker-entrypoint.sh"])),
        cmd: Some(exec(&["postgres"])),
    }
}

/// `python:<version>[-slim|-bookworm|...]`: that Python as `python`, `python3` and `pip` (uv's
/// standalone builds, any 3.x); Python 2.7 is Debian 11's own.
fn python(tag: &str, wanted: Option<&str>) -> Base {
    let (version, _) = version_and_variants(tag);
    let version = if version == "latest" || version == "3" {
        "3.13".to_owned()
    } else {
        minor(&version)
    };
    let os = if version.starts_with('2') {
        "debian@11".to_owned()
    } else {
        Families::AnyGlibc.pick(wanted, "debian@12")
    };
    Base {
        os,
        prelude: vec![PYTHON.replace("{VERSION}", &version)],
        env: vec![("PIP_BREAK_SYSTEM_PACKAGES".into(), "1".into()), ("PYTHONUNBUFFERED".into(), "1".into())],
        workdir: None,
        user: None,
        entrypoint: None,
        // The image's command is Python's interactive shell: on a VM, nothing runs.
        cmd: None,
    }
}

/// `node:<version>`: Node.js of that version (nodejs.org's release, its checksum checked).
fn node(tag: &str, wanted: Option<&str>) -> Base {
    let (version, _) = version_and_variants(tag);
    let version = if version == "latest" || version == "lts" { "22".to_owned() } else { version };
    Base {
        os: Families::AnyGlibc.pick(wanted, "debian@12"),
        prelude: vec![NODE.replace("{VERSION}", &version)],
        env: Vec::new(),
        workdir: None,
        user: None,
        entrypoint: None,
        // The image's command is Node's interactive shell: on a VM, nothing runs.
        cmd: None,
    }
}

/// The exec form of `argv`.
fn exec(argv: &[&str]) -> Command {
    Command::Exec(argv.iter().map(|a| (*a).to_owned()).collect())
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
            assert_eq!(of(image, None).unwrap().os, os, "{image}");
        }
        assert!(matches!(of("alpine:3.7", None), Err(NoEntry::Refused { reason, .. }) if reason.contains("`alpine:3.7`")));
    }

    #[test]
    fn service_images_pin_their_version() {
        let php = of("php:7.4-apache", None).unwrap();
        assert!(php.prelude[0].contains("php7.4-cli") && php.prelude[0].contains("libapache2-mod-php7.4"));
        assert_eq!(php.cmd, Some(exec(&["apache2-foreground"])));
        assert!(of("python:3.8-slim", None).unwrap().prelude[0].contains("uv python install 3.8"));
        assert_eq!(of("python:2.7.18-slim", None).unwrap().os, "debian@11");
        assert!(of("mariadb:10.6.3", None).unwrap().prelude[0].contains("mariadb-10.6"));
        assert!(of("postgres:10.5", None).unwrap().prelude[0].contains("postgresql-10 "));
    }

    #[test]
    fn mysql_is_oracles_build_of_the_release_the_tag_names() {
        let five = of("mysql:5.7", None).unwrap();
        assert_eq!(five.os, "debian@11");
        assert!(five.prelude[0].contains("mysql-5.7.44-linux-glibc2.12-x86_64.tar.gz"));
        assert!(of("mysql:5.7.44", None).unwrap().prelude[0].contains("mysql-5.7.44-"));
        assert!(of("mysql:8.0.33", None).unwrap().prelude[0].contains("mysql-8.0.33-linux-glibc2.17"));
        assert_eq!(of("mysql:8.4", None).unwrap().os, "debian@12");
    }

    #[test]
    fn an_entry_installs_on_the_machines_own_os_when_it_can() {
        assert_eq!(of("mariadb:10.11", Some("rocky@9")).unwrap().os, "rocky@9");
        assert_eq!(of("postgres:16", Some("ubuntu@24.04")).unwrap().os, "ubuntu@24.04");
        // PHP's archive is Debian's: its default instead.
        assert_eq!(of("php:8.2-apache", Some("rocky@9")).unwrap().os, "debian@12");
    }

    #[test]
    fn an_image_without_an_entry_is_unknown_and_scratch_is_refused() {
        assert_eq!(of("vulhub/tomcat:8.5", None), Err(NoEntry::Unknown("vulhub/tomcat".into())));
        assert!(matches!(of("scratch", None), Err(NoEntry::Refused { .. })));
    }
}
