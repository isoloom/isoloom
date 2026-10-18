//! The hosts reached over SSH, and how: the `ISOLOOM_LINUX_*` and `ISOLOOM_WIN_*` variables of
//! the environment, over those of the hosts file, over the defaults (jarvis and karen).
//!
//! The hosts file is `~/.isoloom-lab/hosts.env`, or the one `ISOLOOM_HOSTS_ENV` names. It holds
//! `NAME=value` lines, with `export`, quotes and `#` comments allowed, so a shell can source it
//! too. For each host (`LINUX`, `WIN`): `_USER`, `_HOST`, `_PORT` (SSH's port, else 22 or the
//! SSH config's), `_DIR` (its copies and build folders) and `_JOBS` (its `CARGO_BUILD_JOBS`, else
//! this machine's, else 4).

use std::collections::BTreeMap;
use std::fmt;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;

/// The jobs of a host when nothing sets them.
const DEFAULT_JOBS: NonZeroU32 = match NonZeroU32::new(4) {
    Some(jobs) => jobs,
    None => NonZeroU32::MIN,
};

/// A host reached over SSH: who logs in where, and what its part builds with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SshHost {
    /// The account.
    pub user: String,
    /// The address or name.
    pub host: String,
    /// SSH's port, when it isn't the default.
    pub port: Option<NonZeroU16>,
    /// The folder of its copies and build folders, when set.
    pub dir: Option<String>,
    /// Its `CARGO_BUILD_JOBS`.
    pub jobs: NonZeroU32,
}

impl SshHost {
    /// `user@host`, as ssh and scp take it.
    pub(crate) fn target(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    /// The option that sets the port: `-p` for ssh, `-P` for scp; nothing for the default.
    pub(crate) fn port_args(&self, flag: &'static str) -> Vec<String> {
        self.port.map(|p| vec![flag.to_owned(), p.to_string()]).unwrap_or_default()
    }
}

/// The Linux and the Windows host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hosts {
    /// The Linux host.
    pub linux: SshHost,
    /// The Windows host.
    pub windows: SshHost,
}

/// A setting that can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsError {
    /// A line of the hosts file that isn't `NAME=value`, a comment or blank.
    Line {
        /// Its number, from 1.
        number: usize,
        /// The line.
        text: String,
    },
    /// A port or a job count that isn't a positive number.
    Value {
        /// The variable.
        name: String,
        /// Its value.
        value: String,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Line { number, text } => {
                write!(f, "line {number} is not NAME=value, a comment or blank: {text}")
            }
            SettingsError::Value { name, value } => write!(f, "{name} must be a positive number, not {value:?}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// The `NAME=value` pairs of a hosts file.
///
/// # Errors
///
/// A line that is neither `NAME=value` (optionally after `export`), a `#` comment nor blank.
pub(crate) fn parse_file(text: &str) -> Result<BTreeMap<String, String>, SettingsError> {
    let mut vars = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let assignment = line
            .strip_prefix("export")
            .filter(|rest| rest.starts_with([' ', '\t']))
            .map_or(line, str::trim_start);
        let parsed = assignment.split_once('=').filter(|(name, _)| is_name(name));
        let Some((name, value)) = parsed else {
            return Err(SettingsError::Line {
                number: index + 1,
                text: raw.to_owned(),
            });
        };
        vars.insert(name.to_owned(), unquote(value));
    }
    Ok(vars)
}

/// A shell variable name: a letter or `_`, then letters, digits and `_`.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A value as the shell reads it: inside matching quotes, else up to a ` #` comment.
fn unquote(value: &str) -> String {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|v| v.strip_suffix(quote)) {
            return inner.to_owned();
        }
    }
    value.split(" #").next().unwrap_or_default().trim_end().to_owned()
}

/// The two hosts from `vars` (the environment over the hosts file).
///
/// # Errors
///
/// A port or a job count that isn't a positive number.
pub(crate) fn hosts(vars: &BTreeMap<String, String>) -> Result<Hosts, SettingsError> {
    let get = |name: &str| vars.get(name).filter(|v| !v.is_empty()).cloned();
    let default_jobs = number::<NonZeroU32>(vars, "CARGO_BUILD_JOBS")?.unwrap_or(DEFAULT_JOBS);
    let host = |prefix: &str, address: &str| -> Result<SshHost, SettingsError> {
        let var = |field: &str| format!("ISOLOOM_{prefix}_{field}");
        Ok(SshHost {
            user: get(&var("USER")).unwrap_or_else(|| "florianamette".to_owned()),
            host: get(&var("HOST")).unwrap_or_else(|| address.to_owned()),
            port: number(vars, &var("PORT"))?,
            dir: get(&var("DIR")),
            jobs: number(vars, &var("JOBS"))?.unwrap_or(default_jobs),
        })
    };
    Ok(Hosts {
        linux: host("LINUX", "10.0.0.10")?,
        windows: host("WIN", "10.0.0.40")?,
    })
}

/// `name`'s value as a positive number, when it is set.
fn number<T: std::str::FromStr>(vars: &BTreeMap<String, String>, name: &str) -> Result<Option<T>, SettingsError> {
    match vars.get(name).map(|v| v.trim()).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(value) => value.parse().map(Some).map_err(|_| SettingsError::Value {
            name: name.to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// The hosts file this run reads: `ISOLOOM_HOSTS_ENV`, else `~/.isoloom-lab/hosts.env`.
fn hosts_file() -> Option<PathBuf> {
    std::env::var_os("ISOLOOM_HOSTS_ENV").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| PathBuf::from(home).join(".isoloom-lab/hosts.env"))
    })
}

/// The hosts as this run sees them: the environment over the hosts file (when there is one).
///
/// # Errors
///
/// The hosts file can't be read, or a setting is malformed (the message names the file).
pub(crate) fn load() -> crate::Res<Hosts> {
    let mut vars = BTreeMap::new();
    if let Some(file) = hosts_file().filter(|f| f.is_file()) {
        let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        vars = parse_file(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    vars.extend(std::env::vars());
    Ok(hosts(&vars)?)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn a_hosts_file_reads_like_a_shell_would_source_it() {
        let text = "# the stand-ins\nexport ISOLOOM_LINUX_HOST=10.0.0.30\n\nISOLOOM_WIN_USER=\"vagrant\"\n\
                    ISOLOOM_WIN_DIR='C:/check all'\nISOLOOM_WIN_PORT=2242  # the forwarded port\n";
        let parsed = parse_file(text).unwrap();
        assert_eq!(
            parsed,
            vars(&[
                ("ISOLOOM_LINUX_HOST", "10.0.0.30"),
                ("ISOLOOM_WIN_DIR", "C:/check all"),
                ("ISOLOOM_WIN_PORT", "2242"),
                ("ISOLOOM_WIN_USER", "vagrant"),
            ])
        );
    }

    #[test]
    fn a_line_that_isnt_an_assignment_is_refused_with_its_number() {
        let err = parse_file("ISOLOOM_WIN_HOST=1.2.3.4\nnot a setting\n").unwrap_err();
        assert_eq!(
            err,
            SettingsError::Line {
                number: 2,
                text: "not a setting".to_owned()
            }
        );
        assert!(parse_file("1BAD=x").is_err());
        assert!(parse_file("exported=x").is_ok());
    }

    #[test]
    fn without_settings_the_hosts_are_jarvis_and_karen_on_four_jobs() {
        let h = hosts(&BTreeMap::new()).unwrap();
        assert_eq!(h.linux.target(), "florianamette@10.0.0.10");
        assert_eq!(h.windows.target(), "florianamette@10.0.0.40");
        assert_eq!((h.linux.port, h.windows.dir.as_deref()), (None, None));
        assert_eq!((h.linux.jobs.get(), h.windows.jobs.get()), (4, 4));
    }

    #[test]
    fn each_host_takes_its_own_jobs_and_port_over_this_machines_jobs() {
        let h = hosts(&vars(&[
            ("CARGO_BUILD_JOBS", "6"),
            ("ISOLOOM_LINUX_JOBS", "2"),
            ("ISOLOOM_WIN_PORT", "2242"),
            ("ISOLOOM_WIN_USER", "vagrant"),
            ("ISOLOOM_WIN_HOST", "127.0.0.1"),
        ]))
        .unwrap();
        assert_eq!((h.linux.jobs.get(), h.windows.jobs.get()), (2, 6));
        assert_eq!(h.windows.target(), "vagrant@127.0.0.1");
        assert_eq!(h.windows.port_args("-P"), ["-P", "2242"]);
        assert_eq!(h.linux.port_args("-p"), Vec::<String>::new());
    }

    #[test]
    fn a_port_or_job_count_that_isnt_a_positive_number_is_refused() {
        for (name, value) in [("ISOLOOM_WIN_PORT", "ssh"), ("ISOLOOM_LINUX_JOBS", "0"), ("ISOLOOM_WIN_PORT", "70000")] {
            let err = hosts(&vars(&[(name, value)])).unwrap_err();
            assert_eq!(
                err,
                SettingsError::Value {
                    name: name.to_owned(),
                    value: value.to_owned()
                }
            );
        }
    }

    proptest! {
        #[test]
        fn any_plain_assignment_reads_back(name in "[A-Za-z_][A-Za-z0-9_]{0,20}", value in "[A-Za-z0-9_./:@-]{0,30}") {
            let parsed = parse_file(&format!("{name}={value}\n")).unwrap();
            prop_assert_eq!(parsed.get(&name), Some(&value));
            let quoted = parse_file(&format!("export {name}=\"{value} x\"\n")).unwrap();
            prop_assert_eq!(quoted.get(&name), Some(&format!("{value} x")));
        }
    }
}
