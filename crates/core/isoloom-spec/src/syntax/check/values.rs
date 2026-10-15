//! Where a machine's values come from: its environment (fixed texts, non-sensitive inputs; never
//! a secret or a sensitive input), its files (each source declared, paths unique, owner and mode
//! only on Linux) and its DNS servers (addresses, or machines sharing a network).

use std::collections::BTreeSet;
use std::net::IpAddr;

use super::super::Spec;
use super::super::files::FileSource;
use super::super::machine::Machine;
use super::super::names::is_variable;
use super::super::problem::{Code, FieldPath, Problem, Problems};
use super::super::process::EnvValue;

/// A machine's environment, files and DNS servers.
pub(super) fn machine(spec: &Spec, at: &FieldPath, m: &Machine, out: &mut Problems) {
    environment(spec, at, m, out);
    files(spec, at, m, out);
    dns(spec, at, m, out);
}

fn environment(spec: &Spec, at: &FieldPath, m: &Machine, out: &mut Problems) {
    for (var, value) in &m.environment {
        let at = at.key("environment").key(var);
        if !is_variable(var) {
            out.push(Problem::new(
                Code::InvalidValue,
                at.clone(),
                format!("`{var}` isn't an environment variable's name (letters, digits and `_`, not starting with a digit)"),
            ));
        }
        match value {
            EnvValue::Text(_) => {}
            EnvValue::Secret(s) => out.push(
                Problem::new(
                    Code::SensitiveValue,
                    at,
                    "a secret would show in the machine's environment (`docker inspect`); secrets are files",
                )
                .fix(format!("`files: [{{ path: /run/secrets/{s}, secret: {s} }}]`")),
            ),
            EnvValue::Input(name) => match spec.inputs.get(name) {
                None => out.push(
                    Problem::new(Code::UnknownReference, at, format!("no input named `{name}`"))
                        .fix(format!("declare it: `inputs: {{ {name}: {{ sensitive: false }} }}`")),
                ),
                Some(i) if i.sensitive => out.push(
                    Problem::new(Code::SensitiveValue, at, format!("`{name}` is sensitive and would show in `docker inspect`")).fix(format!(
                        "`files: [{{ path: <path>, input: {name} }}]`, or `inputs: {{ {name}: {{ sensitive: false }} }}`"
                    )),
                ),
                Some(_) if name.as_str() != var => out.push(
                    Problem::new(
                        Code::Refused,
                        at,
                        format!("an input keeps its own name in the environment for now: this variable would be `{name}`, not `{var}`"),
                    )
                    .fix(format!("`{name}: {{ input: {name} }}`, and read `{name}` in the app")),
                ),
                Some(_) => {}
            },
        }
    }
}

fn files(spec: &Spec, at: &FieldPath, m: &Machine, out: &mut Problems) {
    let windows = m.os.is_some_and(super::super::os::OsRef::is_windows);
    let mut paths = BTreeSet::new();
    for (i, f) in m.files.iter().enumerate() {
        let at = at.key("files").index(i);
        if !paths.insert(f.path.as_str()) {
            out.push(Problem::new(
                Code::Conflict,
                at.key("path"),
                format!("`{}` is placed twice on this machine", f.path),
            ));
        }
        let absolute = if windows {
            crate::model::is_drive_path(&f.path) || f.path.starts_with('/')
        } else {
            f.path.starts_with('/')
        };
        if !absolute {
            let want = if windows { "a drive path (`C:\\app\\flag.txt`)" } else { "an absolute path" };
            out.push(Problem::new(Code::InvalidValue, at.key("path"), format!("`{}`: a file goes at {want}", f.path)));
        }
        if windows {
            for (key, set) in [("uid", f.uid.is_some()), ("gid", f.gid.is_some()), ("mode", f.mode.is_some())] {
                if set {
                    out.push(
                        Problem::new(
                            Code::InvalidValue,
                            at.key(key),
                            "Windows has no owner ids or modes: a value file is readable by SYSTEM and Administrators",
                        )
                        .fix("widen it in the machine's own steps"),
                    );
                }
            }
        }
        match &f.source {
            FileSource::Input(name) if spec.inputs.get(name).is_none() => {
                out.push(
                    Problem::new(Code::UnknownReference, at.key("input"), format!("no input named `{name}`")).fix(format!("declare it: `inputs: [{name}]`")),
                );
            }
            FileSource::Secret(name) if !spec.secrets.contains_key(name) => {
                out.push(
                    Problem::new(Code::UnknownReference, at.key("secret"), format!("no secret named `{name}`"))
                        .fix(format!("declare it: `secrets: {{ {name}: {{ env: <VARIABLE> }} }}`")),
                );
            }
            _ => {}
        }
    }
}

/// A DNS server is an address, or a machine sharing a network with this one.
fn dns(spec: &Spec, at: &FieldPath, m: &Machine, out: &mut Problems) {
    let Some(d) = &m.dns else { return };
    for (i, server) in d.servers.iter().enumerate() {
        if server.parse::<IpAddr>().is_ok() {
            continue;
        }
        let at = at.key("dns").key("servers").index(i);
        match spec.machines.get(server) {
            None => out.push(Problem::new(
                Code::UnknownReference,
                at,
                format!("`{server}` is neither an address nor a machine"),
            )),
            Some(other) if !other.networks.keys().any(|n| m.networks.contains_key(n)) => {
                out.push(Problem::new(
                    Code::UnknownReference,
                    at,
                    format!("`{server}` shares no network with this machine, so it can't answer it"),
                ));
            }
            Some(_) => {}
        }
    }
}
