//! The shell rendering of a recipe (design §2.1–§2.2): the POSIX library (`configure.sh`, one
//! function per step kind, for apt, dnf and apk, systemd and OpenRC), and one library call per
//! step. A Linux VM's set-up runs the calls after the library; a generated Dockerfile `RUN`s each
//! call after sourcing it; cloud-init carries the VM's script. Every argument is quoted.

use std::collections::BTreeSet;

use isoloom_plan::configure::{CommandPlan, FilePlan, RepositoryKind, RepositoryPlan, Resource, ServicePlan, ServiceSource, UserPlan};
use isoloom_spec::shell::quote;
use isoloom_spec::syntax::{FileState, Guard, Invocation, Mode};

use super::{Configured, units};
use crate::steps::PROJECT_DIR;
use crate::{GeneratedFile, header};

/// The library, as Isoloom ships it.
pub const LIBRARY: &str = include_str!("shell/configure.sh");

/// Where the library is generated, from the project folder.
pub const LIBRARY_PATH: &str = ".isoloom/configure/lib/configure.sh";

/// The file the restart rule reads on a VM: something changed during this apply.
const CHANGED: &str = "/run/isoloom/configure.changed";

/// The library as a generated file, at [`LIBRARY_PATH`].
#[must_use]
pub fn library_file() -> GeneratedFile {
    GeneratedFile {
        path: LIBRARY_PATH.to_owned(),
        contents: format!("{}{LIBRARY}", header("#")),
    }
}

/// A VM's set-up step for `resources`, a run of the recipe between its scripts and reboots: the
/// library, then each step's call, from the project folder. `fresh`: no script, playbook or
/// reboot ran before it in this apply, so nothing has changed yet; otherwise one may have, and
/// the services after it restart (the rule errs on restarting).
#[must_use]
pub fn script(c: &Configured<'_>, resources: &[Resource], fresh: bool) -> String {
    let calls: String = resources
        .iter()
        .filter_map(|r| call(c, r))
        .map(|line| {
            if literal_dollar(&line) {
                format!("# shellcheck disable=SC2016\n{line}")
            } else {
                line
            }
        })
        .collect();
    let mark = if fresh {
        "rm -f \"$ISOLOOM_CHANGED\"".to_owned()
    } else {
        "mkdir -p /run/isoloom && : >\"$ISOLOOM_CHANGED\"".to_owned()
    };
    let library = needed(&calls);
    format!("(\nset -eu\n{library}ISOLOOM_CHANGED={CHANGED}\n{mark}\ncd {PROJECT_DIR}\n{calls})\n")
}

/// The library's functions `calls` need: those they call, and those these call in turn, in the
/// library's order and without its comments. A VM's step stays small, and holds no function
/// shellcheck would find never called.
fn needed(calls: &str) -> String {
    let functions = functions(&super::compact(LIBRARY));
    let mut wanted: BTreeSet<String> = called(calls);
    loop {
        let more: BTreeSet<String> = functions
            .iter()
            .filter(|(name, _)| wanted.contains(name))
            .flat_map(|(_, text)| called(text))
            .collect();
        let before = wanted.len();
        wanted.extend(more);
        if wanted.len() == before {
            break;
        }
    }
    functions
        .into_iter()
        .filter(|(name, _)| wanted.contains(name))
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .concat()
}

/// The library's functions, by name, each with its text: from its `isoloom_<name>() {` line to
/// the `}` that closes it, both at the start of a line.
fn functions(library: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in library.lines() {
        if let Some((name, text)) = open.as_mut() {
            text.push_str(line);
            text.push('\n');
            if line == "}" {
                out.push((std::mem::take(name), std::mem::take(text)));
                open = None;
            }
        } else if let Some(name) = line.strip_suffix("() {").filter(|n| n.starts_with("isoloom_")) {
            open = Some((name.to_owned(), format!("{line}\n")));
        }
    }
    out
}

/// The library's functions `text` names (`isoloom_<name>`).
fn called(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("isoloom_") {
        let tail = rest.get(at..).unwrap_or_default();
        let len = tail
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(tail.len());
        out.insert(tail.get(..len).unwrap_or_default().to_owned());
        rest = tail.get(len..).unwrap_or_default();
    }
    out
}

/// Step `r`'s library call(s), one per line, as a VM runs it (a template's text inline); `None`
/// for a step the library doesn't run (a script, a reboot, a playbook: the set-up's own steps)
/// or one with nothing to do (no port to open).
#[must_use]
pub fn call(c: &Configured<'_>, r: &Resource) -> Option<String> {
    let line = match r {
        Resource::Repository(p) => repository(p),
        Resource::Packages(ps) => words(
            "isoloom_packages",
            ps.iter().map(|p| match &p.version {
                Some(v) => format!("{}={v}", p.name),
                None => p.name.to_string(),
            }),
        ),
        Resource::Copy(cp) => words(
            "isoloom_copy",
            [cp.from.to_string(), cp.to.to_string(), opt(cp.owner.as_ref()), String::new(), mode(cp.mode)],
        ),
        Resource::Template(t) => {
            let text = c.template(&t.from);
            let mode = t.mode.unwrap_or(Mode::CONFIG);
            match text {
                Ok(text) => words(
                    "isoloom_template",
                    [
                        t.to.to_string(),
                        opt(t.owner.as_ref()),
                        opt(t.group.as_ref()),
                        mode.to_string(),
                        isoloom_spec::shell::base64(text.as_bytes()),
                    ],
                ),
                Err(why) => words("isoloom_fail", [why]),
            }
        }
        Resource::File(f) => file(f),
        Resource::User(u) => user(c, u),
        Resource::Group(g) => words(
            "isoloom_group",
            [g.name.to_string(), g.gid.map(|id| id.to_string()).unwrap_or_default(), yes(g.system)],
        ),
        Resource::Service(s) => service(c, s),
        Resource::Command(cmd) => command(cmd),
        Resource::Firewall(ports) if !ports.is_empty() => words(
            "isoloom_firewall",
            ports.iter().map(|p| format!("{}/{}", p.port, if p.protocol.is_tcp() { "tcp" } else { "udp" })),
        ),
        _ => return None,
    };
    Some(format!("{line}\n"))
}

/// Whether a call holds a `$` or a backquote inside its single quotes (a password's hash, a
/// command's text): literal on purpose, which shellcheck's SC2016 would question.
#[must_use]
pub(super) fn literal_dollar(line: &str) -> bool {
    line.contains(['$', '`'])
}

/// `function` and its arguments, each quoted.
pub(super) fn words<I: IntoIterator<Item = S>, S: AsRef<str>>(function: &str, args: I) -> String {
    let mut line = function.to_owned();
    for a in args {
        line.push(' ');
        line.push_str(&quote(a.as_ref()));
    }
    line
}

/// An optional field as an argument: empty when it isn't given.
fn opt<T: ToString>(v: Option<&T>) -> String {
    v.map(ToString::to_string).unwrap_or_default()
}

/// A mode as an argument: empty when it isn't given.
fn mode(m: Option<Mode>) -> String {
    opt(m.as_ref())
}

/// `yes` or `no`.
fn yes(b: bool) -> String {
    (if b { "yes" } else { "no" }).to_owned()
}

/// A repository: its kind, name, address, key and priority, then apt's suite and components.
fn repository(p: &RepositoryPlan) -> String {
    let kind = match p.kind {
        RepositoryKind::Rpm => "rpm",
        RepositoryKind::Apk => "apk",
        _ => "apt",
    };
    let mut args = vec![
        kind.to_owned(),
        p.name.to_string(),
        p.url.to_string(),
        p.key.url.to_string(),
        p.key.sha256.to_string(),
        p.priority.map(|n| n.to_string()).unwrap_or_default(),
    ];
    if p.kind == RepositoryKind::Apt {
        args.push(opt(p.suite.as_ref()));
        args.extend(p.components.iter().map(ToString::to_string));
    }
    words("isoloom_repository", args)
}

/// A file's state, then its owner, group and mode.
fn file(f: &FilePlan) -> String {
    let mut args = vec![f.path.to_string(), opt(f.owner.as_ref()), opt(f.group.as_ref()), mode(f.mode)];
    match &f.state {
        FileState::Directory => args.push("directory".into()),
        FileState::Absent => args.push("absent".into()),
        FileState::Link(target) => args.extend(["link".into(), target.to_string()]),
        FileState::Download { url, sha256 } => args.extend(["download".into(), url.to_string(), sha256.to_string()]),
        _ => return words("isoloom_fail", ["this Isoloom can't render that file state"]),
    }
    words("isoloom_file", args)
}

/// A user, its literal password hashed for the machine.
fn user(c: &Configured<'_>, u: &UserPlan) -> String {
    let groups: Vec<String> = u.groups.iter().map(ToString::to_string).collect();
    let hash = u.password.as_ref().map(|p| c.password(u.name.as_str(), p.as_str())).unwrap_or_default();
    words(
        "isoloom_user",
        [
            u.name.to_string(),
            u.uid.map(|id| id.to_string()).unwrap_or_default(),
            opt(u.group.as_ref()),
            groups.join(","),
            opt(u.home.as_ref()),
            opt(u.shell.as_ref()),
            yes(u.system),
            hash,
        ],
    )
}

/// A service: the unit Isoloom defines first, when it defines one, then its state.
fn service(c: &Configured<'_>, s: &ServicePlan) -> String {
    let state = words("isoloom_service", [s.unit.to_string(), yes(s.enabled)]);
    match &s.source {
        ServiceSource::Defined { run, user, cwd } => {
            let unit = units::linux(c.recipe().family, &s.unit, run, user.as_ref(), cwd.as_ref());
            format!("{}\n{state}", words("isoloom_unit", [s.unit.to_string(), unit]))
        }
        _ => state,
    }
}

/// A command behind its guard: the guard's kind, its word count and words, then the command's.
fn command(cmd: &CommandPlan) -> String {
    let (kind, guard) = match &cmd.guard {
        Guard::Creates(path) => ("creates", vec![path.to_string()]),
        Guard::Unless(check) => ("unless", argv(check)),
        _ => return words("isoloom_fail", ["this Isoloom can't render that guard"]),
    };
    let args = [kind.to_owned(), guard.len().to_string()].into_iter().chain(guard).chain(argv(&cmd.run));
    words("isoloom_command", args)
}

/// What runs an invocation: a text through `sh -c`, or the program and its arguments.
fn argv(i: &Invocation) -> Vec<String> {
    match i {
        Invocation::Exec(a) => a.args().to_vec(),
        Invocation::Shell(text) => vec!["sh".into(), "-c".into(), text.clone()],
        _ => vec!["false".into()],
    }
}

#[cfg(test)]
mod tests;
