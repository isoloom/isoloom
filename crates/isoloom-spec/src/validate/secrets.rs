//! Secrets: the stores they come from (`secrets:` at the top level) and where each machine
//! places them (`machines.*.secrets`).

use std::collections::BTreeSet;

use super::{Problems, kebab, variable_name};
use crate::model::{MachineSecret, SecretSource, Spec};

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
            p.add(at, "no machine gets it: list it in a machine's `secrets`");
        }
    }
    for (machine, m) in &spec.machines {
        let mut paths = BTreeSet::new();
        for (i, s) in m.secrets.iter().enumerate() {
            let at = format!("machines.{machine}.secrets[{i}]");
            of_machine(spec, &at, s, p);
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

/// One secret of a machine: declared, at a safe path, with a mode a file can have.
fn of_machine(spec: &Spec, at: &str, s: &MachineSecret, p: &mut Problems) {
    let source = s.source();
    if !spec.secrets.contains_key(source) {
        let known: Vec<&str> = spec.secrets.keys().map(String::as_str).collect();
        let field = if matches!(s, MachineSecret::Name(_)) {
            at.to_string()
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
    let path = s.path();
    // The path is a Compose mount target and a Kubernetes mount path: no spaces or ':' (the
    // short syntaxes split on them), no `..`, not a directory.
    if path.split('/').any(|c| c == "..") || path.contains([':', ' ', '\t']) || path.ends_with('/') {
        p.add(
            format!("{at}.target"),
            format!("`{path}` isn't a safe file path inside the machine (no spaces, ':' or '..'), like /etc/app/db-password"),
        );
    }
}
