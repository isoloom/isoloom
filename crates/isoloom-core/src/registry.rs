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
    /// The instance number, when `run --instance` was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<u8>,
    /// The cloud module, for the cloud targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<String>,
    /// The Compose project name, when a tool embedding Isoloom ran the Compose file under its
    /// own (`docker compose -p`); else the file's `name:`. `status`, `connect`, `exec` and
    /// `capture` pass it, or Compose would look for an environment that isn't there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
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
    // Whole or not at all: a reader never sees a half-written file.
    let tmp = p.with_extension(format!("yml.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("can't write {}: {e}", p.display()))
}

/// Reads, changes and writes the registry as one step: `run`, `down` and tools embedding
/// Isoloom (a launcher and its background workers) write it at the same time, and a plain
/// load-then-save lets one of two writers drop the other's entry. Held by a lock file beside
/// it; one left by a process that died is taken over after 30 seconds.
pub fn update(f: impl FnOnce(&mut Registry)) -> Result<(), String> {
    let lock = home().join("status.lock");
    if let Some(parent) = lock.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("can't create {}: {e}", parent.display()))?;
    }
    let started = std::time::Instant::now();
    loop {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::metadata(&lock)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > std::time::Duration::from_secs(30));
                if stale || started.elapsed() > std::time::Duration::from_secs(10) {
                    let _ = std::fs::remove_file(&lock);
                    continue;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(format!("can't lock {}: {e}", lock.display())),
        }
    }
    let result = load().and_then(|mut reg| {
        f(&mut reg);
        save(&reg)
    });
    let _ = std::fs::remove_file(&lock);
    result
}

impl Registry {
    /// Adds or replaces the entry for this project, target and instance.
    pub fn upsert(&mut self, entry: Entry) {
        self.environments
            .retain(|e| !(e.dir == entry.dir && e.target == entry.target && e.instance == entry.instance));
        self.environments.push(entry);
    }

    /// Drops the entry for this project, target and instance; whether there was one.
    pub fn remove(&mut self, dir: &Path, target: Target, instance: Option<u8>) -> bool {
        let before = self.environments.len();
        self.environments.retain(|e| !(e.dir == dir && e.target == target && e.instance == instance));
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
            instance: None,
            cloud: None,
            project: None,
            started: rfc3339(0),
        };
        r.upsert(e(Target::Docker));
        r.upsert(e(Target::Vagrant));
        r.upsert(e(Target::Docker));
        assert_eq!(r.environments.len(), 2);
        assert!(r.remove(Path::new("/p"), Target::Docker, None));
        assert!(!r.remove(Path::new("/p"), Target::Docker, None));
        assert_eq!(r.for_dir(Path::new("/p")).len(), 1);
    }

    #[test]
    fn concurrent_updates_keep_every_entry() {
        let home = std::env::temp_dir().join(format!("isoloom-reg-{}", std::process::id()));
        // SAFETY: the only test touching ISOLOOM_HOME.
        unsafe { std::env::set_var("ISOLOOM_HOME", &home) };
        let threads: Vec<_> = (0..16)
            .map(|i| {
                std::thread::spawn(move || {
                    update(|r| {
                        r.upsert(Entry {
                            name: format!("e{i}"),
                            dir: PathBuf::from(format!("/p{i}")),
                            target: Target::Docker,
                            instance: None,
                            cloud: None,
                            project: None,
                            started: rfc3339(0),
                        })
                    })
                    .unwrap()
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load().unwrap().environments.len(), 16);
        assert!(!home.join("status.lock").exists());
        unsafe { std::env::remove_var("ISOLOOM_HOME") };
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_tools_compose_project_is_kept_and_older_files_still_read() {
        let old = "environments:\n- name: x\n  dir: /p\n  target: docker\n  started: 1970-01-01T00:00:00Z\n";
        let r: Registry = serde_yaml_ng::from_str(old).unwrap();
        assert_eq!(r.environments[0].project, None);
        let mut e = r.environments[0].clone();
        e.project = Some("cyberctf-abc".into());
        let text = serde_yaml_ng::to_string(&Registry { environments: vec![e] }).unwrap();
        assert!(text.contains("project: cyberctf-abc"), "{text}");
        assert_eq!(
            serde_yaml_ng::from_str::<Registry>(&text).unwrap().environments[0].project.as_deref(),
            Some("cyberctf-abc")
        );
    }
}
