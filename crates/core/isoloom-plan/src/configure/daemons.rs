//! The daemon table: for each daemon, per family, its unit, the package that ships it, and the
//! foreground form a container runs it in. A `service: <unit>` step without `run` takes its
//! container form from here, and a recipe that installs a daemon's package without a `service`
//! step for it gets a warning (a daemon runs only where a step says so).
//!
//! The catalogue fills the table: each entry's `daemons:`, and `catalogue/daemons.yml` for the
//! daemons no entry owns.

use isoloom_catalogue::{Catalogue, LogStream};
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

impl DaemonTable {
    /// A table of `entries`.
    #[must_use]
    pub fn new(entries: Vec<DaemonEntry>) -> DaemonTable {
        DaemonTable { entries }
    }

    /// The daemons of the catalogue built into Isoloom (empty only if it doesn't load, which
    /// its own tests rule out).
    #[must_use]
    pub fn common() -> DaemonTable {
        Catalogue::embedded().map(DaemonTable::from).unwrap_or_default()
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

impl From<&Catalogue> for DaemonTable {
    /// The catalogue's daemon table: each daemon on each family its unit exists on.
    fn from(catalogue: &Catalogue) -> DaemonTable {
        let entries = catalogue
            .daemons()
            .map(|(key, d)| DaemonEntry {
                key: key.clone(),
                forms: d
                    .unit
                    .families()
                    .into_iter()
                    .filter_map(|family| {
                        let daemon = Daemon {
                            unit: d.unit.on(family)?.clone(),
                            foreground: d.foreground.on(family)?.clone(),
                            user: d.user.clone(),
                            logs: d
                                .logs
                                .iter()
                                .map(|(path, stream)| LogLink {
                                    path: path.clone(),
                                    stream: (*stream).into(),
                                })
                                .collect(),
                        };
                        let package = d.package.as_ref().and_then(|p| p.on(family)).cloned();
                        Some(FamilyDaemon { family, package, daemon })
                    })
                    .collect(),
            })
            .collect();
        DaemonTable::new(entries)
    }
}

impl From<LogStream> for Stream {
    fn from(s: LogStream) -> Stream {
        match s {
            LogStream::Stderr => Stream::Stderr,
            _ => Stream::Stdout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(s: &str) -> UnitName {
        s.parse().unwrap()
    }

    #[test]
    fn the_catalogue_fills_the_table() {
        let table = DaemonTable::common();
        let keys: Vec<&str> = table.entries().iter().map(|e| e.key.as_str()).collect();
        for key in ["nginx", "apache", "ssh", "cron", "rsyslog"] {
            assert!(keys.contains(&key), "{key}: {keys:?}");
        }
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
