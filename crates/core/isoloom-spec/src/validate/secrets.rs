//! Secrets: the stores they come from (`secrets:` at the top level) and where each machine
//! places them (`machines.*.secrets`).

use std::collections::BTreeSet;

use super::{Problems, kebab, variable_name};
use crate::model::{MachineSecret, SecretSource, Spec, after_drive};

/// The declared secrets, then every machine's.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, source) in &spec.secrets {
        let at = format!("secrets.{name}");
        if !kebab(name) {
            p.add(&at, "secret names are kebab-case: lowercase letters, digits and dashes (e.g. db-password)");
        }
        if let Some(why) = source_problem(source) {
            p.add(format!("{at}.{}", source.store()), why);
        }
        if !spec.machines.values().any(|m| m.secrets.iter().any(|s| s.source() == name)) {
            p.add(at, "no machine gets it: place it as a file, `files: [{ path, secret: <name> }]`");
        }
    }
    for (machine, m) in &spec.machines {
        let windows = m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os));
        let mut paths = BTreeSet::new();
        for (i, s) in m.secrets.iter().enumerate() {
            let at = format!("machines.{machine}.secrets[{i}]");
            of_machine(spec, &at, s, p);
            if windows {
                on_windows(&at, s, p);
            } else {
                safe_path(&at, &s.path(), p);
            }
            if !paths.insert(s.path()) {
                p.add(at, format!("`{}` already holds another secret of this machine", s.path()));
            }
        }
    }
}

/// What is wrong with a store's reference, if anything.
fn source_problem(source: &SecretSource) -> Option<String> {
    let r = source.reference();
    match source {
        SecretSource::Env { env: v } if !variable_name(v) => Some(format!(
            "`{v}` isn't an environment variable name (letters, digits and `_`, not starting with a digit)"
        )),
        SecretSource::File { file: f } if f.trim().is_empty() => Some("the file's path: absolute, `~/...`, or relative to the project".into()),
        SecretSource::Vault { .. } | SecretSource::Sops { .. } if !split_reference(r) => Some(format!(
            "`{r}` isn't `<{}>#<key>`, like {}",
            if source.store() == "vault" { "path" } else { "file" },
            if source.store() == "vault" {
                "secret/app#db_password"
            } else {
                "secrets.enc.yaml#db.password"
            }
        )),
        _ => None,
    }
}

/// Whether `r` is `<where>#<key>`, both parts given.
fn split_reference(r: &str) -> bool {
    r.split_once('#').is_some_and(|(at, key)| !at.trim().is_empty() && !key.trim().is_empty())
}

/// One secret of a machine: declared.
fn of_machine(spec: &Spec, at: &str, s: &MachineSecret, p: &mut Problems) {
    let source = s.source();
    if !spec.secrets.contains_key(source) {
        let known: Vec<&str> = spec.secrets.keys().map(String::as_str).collect();
        let field = if matches!(s, MachineSecret::Name(_)) {
            at.to_owned()
        } else {
            format!("{at}.source")
        };
        p.add(
            field,
            if known.is_empty() {
                format!("`{source}` isn't declared: add it under the top-level `secrets` with its store (`{{ env: VAR }}`, `{{ file: path }}`...)")
            } else {
                format!("`{source}` isn't declared in `secrets` ({})", known.join(", "))
            },
        );
    }
}

/// A path a Compose mount target and a Kubernetes mount path can be: no spaces or ':' (the
/// short syntaxes split on them), no `..`, not a directory.
fn safe_path(at: &str, path: &str, p: &mut Problems) {
    if path.split('/').any(|c| c == "..") || path.contains([':', ' ', '\t']) || path.ends_with('/') {
        p.add(
            format!("{at}.target"),
            format!("`{path}` isn't a safe file path inside the machine (no spaces, ':' or '..'), like /etc/app/db-password"),
        );
    }
}

/// A Windows machine's secret: a file path from `/` or on a drive, and no owner or mode (the
/// file is readable by SYSTEM and Administrators only, which a Unix mode can't say).
fn on_windows(at: &str, s: &MachineSecret, p: &mut Problems) {
    let path = s.path();
    let rest = after_drive(&path).unwrap_or(&path);
    if rest.split(['/', '\\']).any(|c| c == "..") || rest.contains(':') || rest.ends_with(['/', '\\']) {
        p.add(
            format!("{at}.target"),
            format!("`{path}` isn't a file path on the machine (no ':' past the drive, no '..'), like C:\\app\\db-password"),
        );
    }
    let m = s.mount();
    if m.uid.is_some() || m.gid.is_some() || m.mode.is_some() {
        p.add(
            at,
            "a Windows machine's secret has no `uid`, `gid` or `mode`: its file is readable by SYSTEM and Administrators only",
        );
    }
}

#[cfg(test)]
mod tests {
    /// The problems of a spec whose machine `m` is given by `machine` (its lines past `networks`).
    fn problems(machine: &str) -> Vec<String> {
        let yaml = format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nsecrets: {{ db: {{ env: DB }} }}\nmachines:\n  m:\n    networks: {{ lan: 10 }}\n{machine}"
        );
        let spec = crate::testing::model(&yaml).expect("parses");
        crate::validate(&spec).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_windows_secret_takes_a_drive_path_and_no_owner() {
        let win = "    vm: { os: windows-server@2022, provision: [s.ps1] }\n";
        let ok = problems(&format!("{win}    secrets: [db, {{ source: db, target: 'C:\\app\\db' }}]\n"));
        assert!(ok.is_empty(), "{ok:?}");
        let owned = problems(&format!("{win}    secrets: [{{ source: db, mode: 0400 }}]\n"));
        assert!(owned.iter().any(|p| p.contains("no `uid`, `gid` or `mode`")), "{owned:?}");
        let up = problems(&format!("{win}    secrets: [{{ source: db, target: 'C:\\app\\..\\db' }}]\n"));
        assert!(up.iter().any(|p| p.contains("isn't a file path")), "{up:?}");
    }

    #[test]
    fn a_linux_secret_keeps_the_mount_rules() {
        let linux = "    vm: { os: debian@12, provision: [s.sh] }\n";
        let drive = problems(&format!("{linux}    secrets: [{{ source: db, target: 'C:\\app\\db' }}]\n"));
        assert!(drive.iter().any(|p| p.contains("isn't a safe file path")), "{drive:?}");
        let owned = problems(&format!("{linux}    secrets: [{{ source: db, mode: 0400, uid: 1000 }}]\n"));
        assert!(owned.is_empty(), "{owned:?}");
    }
}
