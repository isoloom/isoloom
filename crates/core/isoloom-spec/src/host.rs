//! The host a command runs on, as Isoloom reads it: its environment variables, passed in by the
//! program that runs (the command line, the launcher), never read from the process here, so
//! every function of the crate stays a function of its inputs.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// The host's environment variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Host {
    vars: Vec<(OsString, OsString)>,
}

impl Host {
    /// A host with these environment variables (`std::env::vars_os()`).
    #[must_use]
    pub fn new(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Host {
        Host {
            vars: vars.into_iter().collect(),
        }
    }

    /// The value of variable `key`, when it is set.
    fn var(&self, key: &str) -> Option<&OsStr> {
        self.vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_os_str())
    }

    /// The folder Isoloom keeps its own state in: `$ISOLOOM_HOME`, else `.isoloom` in the user's
    /// home (`$HOME`, `%USERPROFILE%`), else in the current folder.
    #[must_use]
    pub fn home(&self) -> PathBuf {
        if let Some(home) = self.var("ISOLOOM_HOME") {
            return PathBuf::from(home);
        }
        let user_home = self
            .var("HOME")
            .or_else(|| self.var("USERPROFILE"))
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        user_home.join(".isoloom")
    }

    /// The variables whose name and value are text, in name order.
    #[must_use]
    pub fn text_vars(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .vars
            .iter()
            .filter_map(|(k, v)| Some((k.to_str()?.to_owned(), v.to_str()?.to_owned())))
            .collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(vars: &[(&str, &str)]) -> Host {
        Host::new(vars.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))))
    }

    #[test]
    fn isoloom_home_wins_then_the_users_home() {
        assert_eq!(host(&[("HOME", "/home/a"), ("ISOLOOM_HOME", "/srv/iso")]).home(), PathBuf::from("/srv/iso"));
        assert_eq!(host(&[("HOME", "/home/a")]).home(), PathBuf::from("/home/a/.isoloom"));
        assert_eq!(host(&[("USERPROFILE", "C:\\Users\\a")]).home(), PathBuf::from("C:\\Users\\a").join(".isoloom"));
        assert_eq!(Host::default().home(), PathBuf::from("./.isoloom"));
    }

    #[test]
    fn text_variables_come_in_name_order() {
        assert_eq!(
            host(&[("B", "2"), ("A", "1")]).text_vars(),
            [("A".to_owned(), "1".to_owned()), ("B".to_owned(), "2".to_owned())]
        );
    }
}
