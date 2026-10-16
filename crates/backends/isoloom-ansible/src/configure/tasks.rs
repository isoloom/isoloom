//! One Ansible task per step kind, from `ansible.builtin` and `ansible.posix` modules only (design
//! `plan/phase4/design.md` §2.2), each reaching the state and behind the guard the shell library
//! reaches and keeps (`isoloom-render::configure::shell`). The resource conformance harness
//! (`cargo xtask conformance --renderer ansible`) proves a task leaves what the shell leaves: it
//! applies the role, then the shell library on top, and the shell changes nothing.
//!
//! A template's placeholders are filled at generation (not by Ansible's `template` module), so the
//! task copies the rendered file, the same bytes every carrier installs. A download, a repository
//! key and a package version are pinned, checked by the module's `checksum` or the exact version.

use isoloom_plan::configure::{CopyPlan, Family, FilePlan, GroupPlan, PinnedPackage, RepositoryKind, RepositoryPlan, Resource, TemplatePlan, UserPlan};
use isoloom_render::configure::Configured;
use isoloom_spec::syntax::{FileState, Mode};
use serde_json::{Value, json};

use super::services;
use super::task::Task;

/// A project path as the role names it on the control node, under the play's `isoloom_project`
/// (the project folder), so `copy` and `script` find it wherever the playbook is run from.
pub(super) fn project(path: &str) -> String {
    format!("{{{{ isoloom_project }}}}/{path}")
}

/// Where a filled-in template is written under the role (`files/rendered/<from>`), and how a
/// `copy` task names it (role-relative, which Ansible resolves in the role's `files/`).
pub(super) fn rendered(from: &str) -> String {
    format!("rendered/{from}")
}

/// Step `r`'s tasks, each notifying the service handlers in `notify` when it can change something;
/// `seq` is the step's index in the recipe, so a guard's or a firewall's registered variables are
/// unique within the role.
#[must_use]
pub(super) fn step_tasks(c: &Configured<'_>, r: &Resource, notify: &[String], seq: usize) -> Vec<Value> {
    // Registers a task sets are prefixed with the role's name (`ansible-lint`'s `var-naming`).
    let vars = format!("isoloom_machine_{}", c.machine());
    match r {
        Resource::Repository(p) => repository(p, notify),
        Resource::Packages(ps) => vec![packages(c.recipe().family, ps, notify)],
        Resource::Copy(cp) => vec![copy(cp, notify)],
        Resource::Template(t) => vec![template(c, t, notify)],
        Resource::File(f) => vec![file(f, notify)],
        Resource::User(u) => vec![user(c, u, notify)],
        Resource::Group(g) => vec![group(g, notify)],
        Resource::Service(s) => services::service(c, s),
        Resource::Command(cmd) => services::command(cmd, notify, &vars, seq),
        Resource::Firewall(ports) => services::firewall(ports, notify, &vars, seq),
        Resource::Run(run) => vec![services::run(run, notify)],
        Resource::Reboot => vec![services::reboot()],
        Resource::Playbook(p) => vec![services::playbook(p, notify)],
        _ => Vec::new(),
    }
}

/// A mode as Ansible wants it: the octal text (`"0644"`), never a bare number.
pub(super) fn mode(m: Option<Mode>) -> Option<Value> {
    m.map(|m| Value::String(m.to_string()))
}

/// Inserts `key: value` into `map` when `value` is set.
fn put<T: ToString>(map: &mut serde_json::Map<String, Value>, key: &str, value: Option<&T>) {
    if let Some(v) = value {
        map.insert(key.to_owned(), Value::String(v.to_string()));
    }
}

/// A repository: its key downloaded and checked, then a deb822 source, a `.repo` file or a
/// `repositories` line, and, for apt with a priority, a pin.
fn repository(p: &RepositoryPlan, notify: &[String]) -> Vec<Value> {
    let sha = Value::String(format!("sha256:{}", p.key.sha256));
    match p.kind {
        RepositoryKind::Apt => apt_repository(p, &sha, notify),
        RepositoryKind::Rpm => rpm_repository(p, &sha, notify),
        RepositoryKind::Apk => apk_repository(p, &sha, notify),
        _ => Vec::new(),
    }
}

/// The key's destination keyring (deb822 and apk read it there).
fn keyring(name: &str) -> String {
    format!("/usr/share/keyrings/isoloom-{name}.asc")
}

/// An apt repository: the key, a deb822 source, and a pin when a priority is given.
fn apt_repository(p: &RepositoryPlan, sha: &Value, notify: &[String]) -> Vec<Value> {
    let key = keyring(p.name.as_str());
    let suite = p.suite.as_ref().map(ToString::to_string).unwrap_or_default();
    let components: Vec<Value> = p.components.iter().map(|c| Value::String(c.to_string())).collect();
    let mut out = vec![
        key_task(p.key.url.as_str(), &key, sha, "0644"),
        Task::new(format!("{}'s repository", p.name))
            .module(
                "ansible.builtin.deb822_repository",
                json!({ "name": format!("isoloom-{}", p.name), "types": ["deb"], "uris": p.url.to_string(),
                        "suites": [suite], "components": components, "signed_by": key, "enabled": true, "state": "present" }),
            )
            .notify(notify)
            .into_value(),
    ];
    if let Some(priority) = p.priority {
        let host = p
            .url
            .to_string()
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        let pref = format!("Package: *\nPin: origin {host}\nPin-Priority: {priority}\n");
        out.push(
            Task::new(format!("{}'s priority", p.name))
                .module(
                    "ansible.builtin.copy",
                    json!({ "content": pref, "dest": format!("/etc/apt/preferences.d/isoloom-{}.pref", p.name), "mode": "0644" }),
                )
                .notify(notify)
                .into_value(),
        );
    }
    out
}

/// An rpm repository: the key, then a `.repo` file.
fn rpm_repository(p: &RepositoryPlan, sha: &Value, notify: &[String]) -> Vec<Value> {
    let key = format!("/etc/pki/rpm-gpg/isoloom-{}.asc", p.name);
    let mut repo = serde_json::Map::new();
    repo.insert("name".to_owned(), Value::String(format!("isoloom-{}", p.name)));
    repo.insert("description".to_owned(), Value::String(p.name.to_string()));
    repo.insert("baseurl".to_owned(), Value::String(p.url.to_string()));
    repo.insert("enabled".to_owned(), Value::Bool(true));
    repo.insert("gpgcheck".to_owned(), Value::Bool(true));
    repo.insert("gpgkey".to_owned(), Value::String(format!("file://{key}")));
    if let Some(priority) = p.priority {
        repo.insert("priority".to_owned(), Value::String(priority.to_string()));
    }
    vec![
        key_task(p.key.url.as_str(), &key, sha, "0644"),
        Task::new(format!("{}'s repository", p.name))
            .module("ansible.builtin.yum_repository", Value::Object(repo))
            .notify(notify)
            .into_value(),
    ]
}

/// An apk repository: the key under `/etc/apk/keys`, then a `repositories` line.
fn apk_repository(p: &RepositoryPlan, sha: &Value, notify: &[String]) -> Vec<Value> {
    let key_name = p.key.url.as_str().rsplit('/').next().unwrap_or("isoloom.rsa.pub").to_owned();
    vec![
        key_task(p.key.url.as_str(), &format!("/etc/apk/keys/{key_name}"), sha, "0644"),
        Task::new(format!("{}'s repository", p.name))
            .module(
                "ansible.builtin.lineinfile",
                json!({ "path": "/etc/apk/repositories", "line": p.url.to_string(), "create": true, "state": "present" }),
            )
            .notify(notify)
            .into_value(),
    ]
}

/// A repository key, downloaded to `dest` and checked against `sha`.
fn key_task(url: &str, dest: &str, sha: &Value, mode: &str) -> Value {
    Task::new(format!("the repository key {dest}"))
        .module("ansible.builtin.get_url", json!({ "url": url, "dest": dest, "checksum": sha, "mode": mode }))
        .into_value()
}

/// The `packages` step: each package at its version, installed with the family's module (a
/// guarded `command` on Alpine, since the `apk` module is in `community.general`).
fn packages(family: Family, ps: &[PinnedPackage], notify: &[String]) -> Value {
    let name = |p: &PinnedPackage| match (&p.version, family) {
        (Some(v), Family::RedHat) => format!("{}-{v}", p.name),
        (Some(v), _) => format!("{}={v}", p.name),
        (None, _) => p.name.to_string(),
    };
    let specs: Vec<String> = ps.iter().map(name).collect();
    let names: Vec<Value> = specs.iter().map(|s| Value::String(s.clone())).collect();
    let task = Task::new(format!("packages {}", specs.join(" ")));
    let task = match family {
        Family::Debian => task.module("ansible.builtin.apt", json!({ "name": names, "update_cache": true, "state": "present" })),
        Family::RedHat => task.module("ansible.builtin.dnf", json!({ "name": names, "state": "present" })),
        Family::Alpine => task
            .module("ansible.builtin.command", json!({ "argv": apk_argv(&specs) }))
            .set("register", json!("isoloom_apk"))
            .set("changed_when", json!("'Installing' in isoloom_apk.stdout or 'Upgrading' in isoloom_apk.stdout")),
        Family::Windows => task.module("ansible.windows.win_feature", json!({ "name": names, "state": "present" })),
    };
    task.notify(notify).into_value()
}

/// `apk add` with the package specs, as an argv so no word is split.
fn apk_argv(specs: &[String]) -> Vec<Value> {
    let mut argv = vec![Value::String("apk".into()), Value::String("add".into()), Value::String("--no-cache".into())];
    argv.extend(specs.iter().map(|s| Value::String(s.clone())));
    argv
}

/// The `copy` step: a project file or folder at its destination, with owner and mode.
fn copy(cp: &CopyPlan, notify: &[String]) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("src".to_owned(), Value::String(project(cp.from.as_str())));
    args.insert("dest".to_owned(), Value::String(cp.to.to_string()));
    put(&mut args, "owner", cp.owner.as_ref());
    // An explicit mode, or `preserve` (the source's), which `cp` keeps too and `ansible-lint`'s
    // `risky-file-permissions` wants set.
    args.insert("mode".to_owned(), mode(cp.mode).unwrap_or_else(|| Value::String("preserve".to_owned())));
    Task::new(format!("copy {} to {}", cp.from, cp.to))
        .module("ansible.builtin.copy", Value::Object(args))
        .notify(notify)
        .into_value()
}

/// The `template` step: the file Isoloom filled in, copied (not Ansible's `template` module, so
/// every carrier installs the same bytes); a template that can't be filled fails the play.
fn template(c: &Configured<'_>, t: &TemplatePlan, notify: &[String]) -> Value {
    if let Err(why) = c.template(&t.from) {
        return Task::new(format!("the template {}", t.from))
            .module("ansible.builtin.fail", json!({ "msg": why }))
            .into_value();
    }
    let mut args = serde_json::Map::new();
    args.insert("src".to_owned(), Value::String(rendered(t.from.as_str())));
    args.insert("dest".to_owned(), Value::String(t.to.to_string()));
    put(&mut args, "owner", t.owner.as_ref());
    put(&mut args, "group", t.group.as_ref());
    args.insert("mode".to_owned(), mode(t.mode).unwrap_or_else(|| Value::String(Mode::CONFIG.to_string())));
    Task::new(format!("template {} to {}", t.from, t.to))
        .module("ansible.builtin.copy", Value::Object(args))
        .notify(notify)
        .into_value()
}

/// The `file` step: a directory, a removal, a link or a pinned download, with owner, group, mode.
fn file(f: &FilePlan, notify: &[String]) -> Value {
    let mut args = serde_json::Map::new();
    let (module, name) = match &f.state {
        FileState::Directory => {
            args.insert("path".to_owned(), Value::String(f.path.to_string()));
            args.insert("state".to_owned(), Value::String("directory".into()));
            ("ansible.builtin.file", format!("the directory {}", f.path))
        }
        FileState::Absent => {
            args.insert("path".to_owned(), Value::String(f.path.to_string()));
            args.insert("state".to_owned(), Value::String("absent".into()));
            ("ansible.builtin.file", format!("{} removed", f.path))
        }
        FileState::Link(target) => {
            args.insert("path".to_owned(), Value::String(f.path.to_string()));
            args.insert("src".to_owned(), Value::String(target.to_string()));
            args.insert("state".to_owned(), Value::String("link".into()));
            args.insert("force".to_owned(), Value::Bool(true));
            ("ansible.builtin.file", format!("the link {}", f.path))
        }
        FileState::Download { url, sha256 } => {
            args.insert("url".to_owned(), Value::String(url.to_string()));
            args.insert("dest".to_owned(), Value::String(f.path.to_string()));
            args.insert("checksum".to_owned(), Value::String(format!("sha256:{sha256}")));
            ("ansible.builtin.get_url", format!("the download {}", f.path))
        }
        _ => {
            return Task::new(format!("the file {}", f.path))
                .module("ansible.builtin.fail", json!({ "msg": "this Isoloom can't render that file state" }))
                .into_value();
        }
    };
    put(&mut args, "owner", f.owner.as_ref());
    put(&mut args, "group", f.group.as_ref());
    if let Some(m) = mode(f.mode) {
        args.insert("mode".to_owned(), m);
    }
    Task::new(name).module(module, Value::Object(args)).notify(notify).into_value()
}

/// The `user` step: a local user, its supplementary groups added (never removed), its literal
/// password hashed as Linux keeps it (the same hash the shell renderer writes).
fn user(c: &Configured<'_>, u: &UserPlan, notify: &[String]) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("name".to_owned(), Value::String(u.name.to_string()));
    if let Some(uid) = u.uid {
        args.insert("uid".to_owned(), Value::Number(uid.into()));
    }
    put(&mut args, "group", u.group.as_ref());
    if !u.groups.is_empty() {
        args.insert(
            "groups".to_owned(),
            Value::Array(u.groups.iter().map(|g| Value::String(g.to_string())).collect()),
        );
        args.insert("append".to_owned(), Value::Bool(true));
    }
    put(&mut args, "home", u.home.as_ref());
    put(&mut args, "shell", u.shell.as_ref());
    args.insert("system".to_owned(), Value::Bool(u.system));
    if let Some(password) = &u.password {
        args.insert("password".to_owned(), Value::String(c.password(u.name.as_str(), password.as_str())));
    }
    Task::new(format!("the user {}", u.name))
        .module("ansible.builtin.user", Value::Object(args))
        .notify(notify)
        .into_value()
}

/// The `group` step: a local group, with its gid and whether it is a system group.
fn group(g: &GroupPlan, notify: &[String]) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("name".to_owned(), Value::String(g.name.to_string()));
    if let Some(gid) = g.gid {
        args.insert("gid".to_owned(), Value::Number(gid.into()));
    }
    args.insert("system".to_owned(), Value::Bool(g.system));
    Task::new(format!("the group {}", g.name))
        .module("ansible.builtin.group", Value::Object(args))
        .notify(notify)
        .into_value()
}
