//! A machine's secret files on hosts that exist already: nothing runs there to deliver them, so
//! the play does, each value read where `ansible-playbook` runs by a lookup of its source (the
//! runner's environment for an environment variable or an input, a file of that computer,
//! `vault` or `sops` there) and written at the file's path with its owner and mode, the task kept
//! out of Ansible's log. The generated files hold the lookups, never a value; what a lookup reads
//! (a variable's name, a path, a command) is a task variable, written `!unsafe`, never part of
//! the template.

use isoloom_plan::{Environment, SecretFile, ValueSource};
use isoloom_spec::model::SecretSource;
use isoloom_spec::shell::quote;
use serde_json::{Value, json};

use super::task::{Task, jinja};

/// The mode a secret's file gets without one of its own: the owner's to read.
const MODE: u32 = isoloom_plan::secrets::VM_DEFAULT_MODE;

/// The play delivering machine `name`'s secret files, when it has any.
pub(super) fn play(env: &Environment, name: &str, project: &Value) -> Option<Value> {
    let m = env.machines.get(name)?;
    if m.secrets.is_empty() {
        return None;
    }
    let tasks: Vec<Value> = m.secrets.iter().flat_map(|f| tasks(env, f)).collect();
    Some(json!({
        "name": format!("Deliver {name}'s secrets"),
        "hosts": name,
        "become": true,
        "vars": { "isoloom_project": project },
        "tasks": tasks,
    }))
}

/// The folder of `f`'s file, then the file from its value.
fn tasks(env: &Environment, f: &SecretFile) -> Vec<Value> {
    let (expression, vars) = lookup(env, f);
    let mut copy = json!({ "content": jinja(expression), "dest": f.path, "mode": format!("{:04o}", f.mode.unwrap_or(MODE)) });
    if let Some(map) = copy.as_object_mut() {
        if let Some(uid) = f.uid {
            map.insert("owner".to_owned(), json!(uid.to_string()));
        }
        if let Some(gid) = f.gid {
            map.insert("group".to_owned(), json!(gid.to_string()));
        }
    }
    let folder = f.path.rsplit_once('/').map(|(dir, _)| dir).filter(|dir| !dir.is_empty());
    let mut out: Vec<Value> = folder
        .map(|dir| {
            Task::new(format!("{}'s folder", f.path))
                .module("ansible.builtin.file", json!({ "path": dir, "state": "directory", "mode": "0755" }))
                .into_value()
        })
        .into_iter()
        .collect();
    out.push(
        Task::new(format!("{} from {}", f.path, f.name))
            .module("ansible.builtin.copy", copy)
            .set("no_log", json!(true))
            .set("vars", vars)
            .into_value(),
    );
    out
}

/// The lookup reading `f`'s value where `ansible-playbook` runs, and the task variables it reads.
fn lookup(env: &Environment, f: &SecretFile) -> (String, Value) {
    let source = match &f.source {
        ValueSource::Input(input) => SecretSource::Env { env: input.clone() },
        ValueSource::Secret(name) => env.spec().secrets.get(name).cloned().unwrap_or(SecretSource::Env { env: name.clone() }),
    };
    match source {
        SecretSource::Env { env } | SecretSource::Input { input: env } => (
            "{{ lookup('ansible.builtin.env', isoloom_secret_env) }}".to_owned(),
            json!({ "isoloom_secret_env": env }),
        ),
        SecretSource::File { file } => file_lookup(&file),
        SecretSource::Vault { vault } => {
            let (path, field) = vault.split_once('#').unwrap_or((vault.as_str(), ""));
            pipe(&format!("vault kv get -field={} {}", quote(field), quote(path)))
        }
        SecretSource::Sops { sops } => {
            let (file, key) = sops.split_once('#').unwrap_or((sops.as_str(), ""));
            let extract: String = key.split('.').map(|k| format!("[\"{k}\"]")).collect();
            // The file is the project's when relative, as the runner reads it.
            let file = if file.starts_with('/') {
                quote(file)
            } else if let Some(rest) = file.strip_prefix("~/") {
                format!("\"$HOME\"/{}", quote(rest))
            } else {
                format!("{{project}}/{}", quote(file))
            };
            pipe(&format!("sops -d --extract {} {file}", quote(&extract)))
        }
    }
}

/// A file of the computer `ansible-playbook` runs on: relative paths from the project folder,
/// `~/…` from the home folder, others as they are.
fn file_lookup(file: &str) -> (String, Value) {
    if let Some(rest) = file.strip_prefix("~/") {
        return (
            "{{ lookup('ansible.builtin.file', lookup('ansible.builtin.env', 'HOME') ~ '/' ~ isoloom_secret_file) }}".to_owned(),
            json!({ "isoloom_secret_file": rest }),
        );
    }
    if file.starts_with('/') {
        return (
            "{{ lookup('ansible.builtin.file', isoloom_secret_file) }}".to_owned(),
            json!({ "isoloom_secret_file": file }),
        );
    }
    (
        "{{ lookup('ansible.builtin.file', isoloom_project ~ '/' ~ isoloom_secret_file) }}".to_owned(),
        json!({ "isoloom_secret_file": file }),
    )
}

/// A command's output where `ansible-playbook` runs (its `{project}` the project folder).
fn pipe(command: &str) -> (String, Value) {
    (
        "{{ lookup('ansible.builtin.pipe', isoloom_secret_command | replace('{project}', isoloom_project | quote)) }}".to_owned(),
        json!({ "isoloom_secret_command": command }),
    )
}
