//! The daemon table: for each daemon, per family, its unit, the package that ships it, and the
//! foreground form a container runs it in. A `service: <unit>` step without `run` takes its
//! container form from here, and a recipe that installs a daemon's package without a `service`
//! step for it gets a warning (a daemon runs only where a step says so).
//!
//! Phase 4.3 fills the table from the catalogue (each entry's `daemons:`, and
//! `catalogue/daemons.yml` for daemons no entry owns). Until then it holds the common daemons
//! below, whose foreground forms are the ones their official images run.

use isoloom_spec::syntax::{AbsPath, Argv, PackageName, UnitName, UserName};

use super::family::Family;

/// A daemon as a container runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Daemon {
    /// Its unit (what a VM enables).
    pub unit: UnitName,
    /// Its foreground form: what the container's runner `exec`s.
    pub foreground: Argv,
    /// The user it drops to (root when `None`).
    pub user: Option<UserName>,
    /// Its log files, linked to the container's output.
    pub logs: Vec<LogLink>,
}

/// A daemon's log file, linked to one of the container's streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLink {
    /// The log file.
    pub path: AbsPath,
    /// The stream it is linked to.
    pub stream: Stream,
}

/// A container's output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Stream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// A daemon on one family: the package that ships it, and how it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyDaemon {
    /// The family.
    pub family: Family,
    /// The package that ships it, when one does.
    pub package: Option<PackageName>,
    /// How it runs.
    pub daemon: Daemon,
}

/// One daemon of the table: its key (what a step names) and its form on each family it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonEntry {
    /// What a `service` step names it by (`ssh`), besides each family's unit (`sshd`).
    pub key: UnitName,
    /// Its form on each family.
    pub forms: Vec<FamilyDaemon>,
}

/// The daemons Isoloom knows the foreground form of.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonTable {
    entries: Vec<DaemonEntry>,
}

/// A daemon written as data: a key, then its forms.
type Seed = (&'static str, &'static [FormSeed]);

/// A daemon's form on a family, written as data: the family, its unit, its package, its
/// foreground form and its log files (`path=stdout` or `path=stderr`).
type FormSeed = (Family, &'static str, &'static str, &'static [&'static str], &'static [&'static str]);

/// The common daemons, until the catalogue fills the table (Phase 4.3).
const SEED: &[Seed] = &[
    (
        "nginx",
        &[
            (Family::Debian, "nginx", "nginx", &["nginx", "-g", "daemon off;"], NGINX_LOGS),
            (Family::RedHat, "nginx", "nginx", &["nginx", "-g", "daemon off;"], NGINX_LOGS),
            (Family::Alpine, "nginx", "nginx", &["nginx", "-g", "daemon off;"], NGINX_LOGS),
        ],
    ),
    (
        "apache",
        &[
            (Family::Debian, "apache2", "apache2", &["apache2ctl", "-D", "FOREGROUND"], &[]),
            (Family::RedHat, "httpd", "httpd", &["httpd", "-D", "FOREGROUND"], &[]),
            (Family::Alpine, "apache2", "apache2", &["httpd", "-D", "FOREGROUND"], &[]),
        ],
    ),
    (
        "ssh",
        &[
            (Family::Debian, "ssh", "openssh-server", &["/usr/sbin/sshd", "-D", "-e"], &[]),
            (Family::RedHat, "sshd", "openssh-server", &["/usr/sbin/sshd", "-D", "-e"], &[]),
            (Family::Alpine, "sshd", "openssh-server", &["/usr/sbin/sshd", "-D", "-e"], &[]),
        ],
    ),
    (
        "cron",
        &[
            (Family::Debian, "cron", "cron", &["cron", "-f"], &[]),
            (Family::RedHat, "crond", "cronie", &["crond", "-n"], &[]),
        ],
    ),
    (
        "rsyslog",
        &[
            (Family::Debian, "rsyslog", "rsyslog", &["rsyslogd", "-n"], &[]),
            (Family::RedHat, "rsyslog", "rsyslog", &["rsyslogd", "-n"], &[]),
            (Family::Alpine, "rsyslog", "rsyslog", &["rsyslogd", "-n"], &[]),
        ],
    ),
];

/// nginx's logs, sent to the container's output as its official image does.
const NGINX_LOGS: &[&str] = &["/var/log/nginx/access.log=stdout", "/var/log/nginx/error.log=stderr"];

impl DaemonTable {
    /// A table of `entries`.
    #[must_use]
    pub fn new(entries: Vec<DaemonEntry>) -> DaemonTable {
        DaemonTable { entries }
    }

    /// The common daemons (nginx, apache, ssh, cron, rsyslog), until the catalogue fills the
    /// table.
    #[must_use]
    pub fn common() -> DaemonTable {
        DaemonTable::new(SEED.iter().filter_map(|(key, forms)| entry(key, forms)).collect())
    }

    /// The entries.
    #[must_use]
    pub fn entries(&self) -> &[DaemonEntry] {
        &self.entries
    }

    /// The daemon a step names `name` on `family`: by the table's key, else by the family's unit.
    #[must_use]
    pub fn find(&self, name: &UnitName, family: Family) -> Option<(&DaemonEntry, &FamilyDaemon)> {
        self.entries.iter().find_map(|e| {
            let form = e.forms.iter().find(|f| f.family == family)?;
            (e.key == *name || form.daemon.unit == *name).then_some((e, form))
        })
    }

    /// The daemon package `package` ships on `family`, when it ships one.
    #[must_use]
    pub fn shipped_by(&self, package: &PackageName, family: Family) -> Option<(&DaemonEntry, &FamilyDaemon)> {
        self.entries.iter().find_map(|e| {
            let form = e.forms.iter().find(|f| f.family == family && f.package.as_ref() == Some(package))?;
            Some((e, form))
        })
    }
}

/// A seed entry, typed: `None` when one of its names breaks its rule (a test proves none does).
fn entry(key: &str, forms: &[FormSeed]) -> Option<DaemonEntry> {
    let forms = forms
        .iter()
        .map(|(family, unit, package, foreground, logs)| {
            let logs = logs
                .iter()
                .map(|l| {
                    let (path, stream) = l.split_once('=')?;
                    let stream = if stream == "stderr" { Stream::Stderr } else { Stream::Stdout };
                    Some(LogLink {
                        path: path.parse().ok()?,
                        stream,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            let daemon = Daemon {
                unit: unit.parse().ok()?,
                foreground: Argv::try_from(foreground.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>()).ok()?,
                user: None,
                logs,
            };
            Some(FamilyDaemon {
                family: *family,
                package: Some(package.parse().ok()?),
                daemon,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(DaemonEntry { key: key.parse().ok()?, forms })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(s: &str) -> UnitName {
        s.parse().unwrap()
    }

    #[test]
    fn every_common_daemon_is_typed() {
        assert_eq!(DaemonTable::common().entries().len(), SEED.len());
    }

    #[test]
    fn a_step_finds_a_daemon_by_key_or_by_the_unit_its_family_names() {
        let table = DaemonTable::common();
        let (_, debian) = table.find(&unit("ssh"), Family::Debian).unwrap();
        assert_eq!(debian.daemon.unit.as_str(), "ssh");
        let (entry, rocky) = table.find(&unit("sshd"), Family::RedHat).unwrap();
        assert_eq!((entry.key.as_str(), rocky.daemon.foreground.program()), ("ssh", "/usr/sbin/sshd"));
        assert!(table.find(&unit("cron"), Family::Alpine).is_none(), "no cron form on Alpine");
        assert!(table.find(&unit("myd"), Family::Debian).is_none());
    }

    #[test]
    fn a_package_names_the_daemon_it_ships() {
        let table = DaemonTable::common();
        let (entry, form) = table.shipped_by(&"httpd".parse().unwrap(), Family::RedHat).unwrap();
        assert_eq!((entry.key.as_str(), form.daemon.unit.as_str()), ("apache", "httpd"));
        assert!(table.shipped_by(&"httpd".parse().unwrap(), Family::Debian).is_none());
        let (_, nginx) = table.shipped_by(&"nginx".parse().unwrap(), Family::Alpine).unwrap();
        assert_eq!(nginx.daemon.logs.len(), 2);
    }
}
