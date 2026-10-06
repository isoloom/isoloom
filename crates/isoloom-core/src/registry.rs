//! The registry of running environments: what `isoloom run` brought up on this host, where
//! and on which target, so `status`, `connect`, `exec` and `capture` know what to talk to
//! without being told. One YAML file, `~/.isoloom/status.yml` (or `$ISOLOOM_HOME/status.yml`),
//! written by `run` and `down`. Tools embedding Isoloom read it through this module.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Target;

/// One running environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The spec's name.
    pub name: String,
    /// The project folder (absolute).
    pub dir: PathBuf,
    pub target: Target,
    /// The cloud module, for the cloud targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<String>,
    /// When `run` finished, as RFC 3339 (UTC).
    pub started: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub environments: Vec<Entry>,
}

/// The folder Isoloom keeps its own state in: `$ISOLOOM_HOME`, else `~/.isoloom`.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("ISOLOOM_HOME") {
        return PathBuf::from(h);
    }
    let user_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    user_home.join(".isoloom")
}

/// The registry file.
pub fn path() -> PathBuf {
    home().join("status.yml")
}

/// Reads the registry; a missing file is an empty registry.
pub fn load() -> Result<Registry, String> {
    let p = path();
    match std::fs::read_to_string(&p) {
        Ok(text) => serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Registry::default()),
        Err(e) => Err(format!("can't read {}: {e}", p.display())),
    }
}

/// Writes the registry (creating the folder).
pub fn save(r: &Registry) -> Result<(), String> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("can't create {}: {e}", parent.display()))?;
    }
    let text = format!(
        "# Environments `isoloom run` brought up on this host. Written by `isoloom run` and `isoloom down`;\n# `isoloom status` reads it.\n{}",
        serde_yaml_ng::to_string(r).map_err(|e| e.to_string())?
    );
    std::fs::write(&p, text).map_err(|e| format!("can't write {}: {e}", p.display()))
}

impl Registry {
    /// Adds or replaces the entry for this project and target.
    pub fn upsert(&mut self, entry: Entry) {
        self.environments.retain(|e| !(e.dir == entry.dir && e.target == entry.target));
        self.environments.push(entry);
    }

    /// Drops the entry for this project and target; whether there was one.
    pub fn remove(&mut self, dir: &Path, target: Target) -> bool {
        let before = self.environments.len();
        self.environments.retain(|e| !(e.dir == dir && e.target == target));
        before != self.environments.len()
    }

    /// The entries for a project folder.
    pub fn for_dir(&self, dir: &Path) -> Vec<&Entry> {
        self.environments.iter().filter(|e| e.dir == dir).collect()
    }
}

/// Now, as RFC 3339 in UTC (seconds).
pub fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339(secs)
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_791_314_400), "2026-10-06T19:20:00Z");
    }

    #[test]
    fn upsert_replaces_the_same_project_and_target() {
        let mut r = Registry::default();
        let e = |t: Target| Entry {
            name: "x".into(),
            dir: PathBuf::from("/p"),
            target: t,
            cloud: None,
            started: rfc3339(0),
        };
        r.upsert(e(Target::Docker));
        r.upsert(e(Target::Vagrant));
        r.upsert(e(Target::Docker));
        assert_eq!(r.environments.len(), 2);
        assert!(r.remove(Path::new("/p"), Target::Docker));
        assert!(!r.remove(Path::new("/p"), Target::Docker));
        assert_eq!(r.for_dir(Path::new("/p")).len(), 1);
    }
}
