//! The PowerShell rendering of a Windows recipe (design §2.1–§2.2): the library
//! (`configure.ps1`, one function per step kind) and one call per step. A Windows VM's set-up
//! runs the calls after the library. Every argument is a single-quoted literal.

use isoloom_plan::configure::{CommandPlan, FilePlan, Resource, ServicePlan, ServiceSource, UserPlan};
use isoloom_spec::shell::base64;
use isoloom_spec::syntax::{FileState, Guard, Invocation};

use super::Configured;
use crate::powershell::quote;
use crate::{GeneratedFile, header};

/// The library, as Isoloom ships it.
pub const LIBRARY: &str = include_str!("powershell/configure.ps1");

/// Where the library is generated, from the project folder.
pub const LIBRARY_PATH: &str = ".isoloom/configure/lib/configure.ps1";

/// Where a Windows VM's project is, and where its recipe runs from.
const PROJECT_DIR: &str = "C:\\isoloom";

/// The file the restart rule reads on a Windows VM.
const CHANGED: &str = "C:\\Windows\\Temp\\isoloom-configure.changed";

/// The library as a generated file, at [`LIBRARY_PATH`].
#[must_use]
pub fn library_file() -> GeneratedFile {
    GeneratedFile {
        path: LIBRARY_PATH.to_owned(),
        contents: format!("{}{LIBRARY}", header("#")),
    }
}

/// A Windows VM's set-up step for `resources` (see [`super::shell::script`]): the library, then
/// each step's call, from the project folder, stopping at the first error.
#[must_use]
pub fn script(c: &Configured<'_>, resources: &[Resource], fresh: bool) -> String {
    let calls: String = resources.iter().filter_map(|r| call(c, r)).collect();
    let mark = if fresh {
        "Remove-Item -LiteralPath $env:ISOLOOM_CHANGED -Force -ErrorAction SilentlyContinue"
    } else {
        "Set-Content -LiteralPath $env:ISOLOOM_CHANGED -Value 'changed'"
    };
    let library = super::compact(LIBRARY);
    format!("$ErrorActionPreference = 'Stop'\n{library}$env:ISOLOOM_CHANGED = '{CHANGED}'\n{mark}\nSet-Location -LiteralPath '{PROJECT_DIR}'\n{calls}")
}

/// Step `r`'s call(s), one per line; `None` for a step the library doesn't run (a script, a
/// reboot, a playbook) or one Windows can't carry (a repository: lowering refuses it).
#[must_use]
pub fn call(c: &Configured<'_>, r: &Resource) -> Option<String> {
    let line = match r {
        Resource::Packages(ps) => format!("Install-IsoloomFeature -Name {}", list(ps.iter().map(|p| p.name.to_string()))),
        Resource::Copy(cp) => format!("Copy-IsoloomItem -From {} -To {}", quote(cp.from.as_str()), quote(cp.to.as_str())),
        Resource::Template(t) => match c.template(&t.from) {
            Ok(text) => format!("Install-IsoloomTemplate -To {} -Base64 '{}'", quote(t.to.as_str()), base64(text.as_bytes())),
            Err(why) => format!("throw {}", quote(&format!("isoloom: {why}"))),
        },
        Resource::File(f) => file(f)?,
        Resource::User(u) => user(u),
        Resource::Group(g) => format!("Add-IsoloomGroup -Name {}", quote(g.name.as_str())),
        Resource::Service(s) => service(s),
        Resource::Command(cmd) => command(cmd),
        Resource::Firewall(ports) => ports
            .iter()
            .map(|p| {
                format!(
                    "Enable-IsoloomFirewallPort -Port {} -Protocol {}",
                    p.port,
                    if p.protocol.is_tcp() { "TCP" } else { "UDP" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    (!line.is_empty()).then(|| format!("{line}\n"))
}

/// A PowerShell array of literals (`'a', 'b'`).
fn list<I: IntoIterator<Item = String>>(items: I) -> String {
    items.into_iter().map(|i| quote(&i)).collect::<Vec<_>>().join(", ")
}

/// A file's state (a link is refused on Windows: lowering reports it).
fn file(f: &FilePlan) -> Option<String> {
    let path = quote(f.path.as_str());
    Some(match &f.state {
        FileState::Directory => format!("Install-IsoloomFile -Path {path} -State directory"),
        FileState::Absent => format!("Install-IsoloomFile -Path {path} -State absent"),
        FileState::Download { url, sha256 } => format!(
            "Install-IsoloomFile -Path {path} -State download -Url {} -Sha256 '{sha256}'",
            quote(url.as_str())
        ),
        _ => return None,
    })
}

/// A local user: its password (plain: Windows takes no hash) and its groups, its primary group
/// first.
fn user(u: &UserPlan) -> String {
    let phrase = u.password.as_ref().map(|p| format!(" -Phrase {}", quote(p.as_str()))).unwrap_or_default();
    let groups: Vec<String> = u.group.iter().chain(&u.groups).map(ToString::to_string).collect();
    let groups = if groups.is_empty() {
        String::new()
    } else {
        format!(" -Groups {}", list(groups))
    };
    format!("Add-IsoloomUser -Name {}{phrase}{groups}", quote(u.name.as_str()))
}

/// A service: a Windows service's state, or, for one Isoloom defines, its startup task's.
fn service(s: &ServicePlan) -> String {
    let name = quote(s.unit.as_str());
    let enabled = if s.enabled { "$true" } else { "$false" };
    match &s.source {
        ServiceSource::Defined { run, cwd, .. } => {
            let dir = cwd.as_ref().map(|d| format!(" -Directory {}", quote(d.as_str()))).unwrap_or_default();
            format!(
                "Register-IsoloomTask -Name {name} -Run {}{dir}\nRegister-IsoloomTaskState -Name {name} -Enabled {enabled}",
                list(run.args().iter().cloned())
            )
        }
        _ => format!("Register-IsoloomService -Name {name} -Enabled {enabled}"),
    }
}

/// A command behind its guard.
fn command(cmd: &CommandPlan) -> String {
    let guard = match &cmd.guard {
        Guard::Creates(path) => format!(" -Creates {}", quote(path.as_str())),
        Guard::Unless(check) => format!(" -Unless {}", quote(&text(check))),
        _ => String::new(),
    };
    format!("Invoke-IsoloomCommand -Run {}{guard}", quote(&text(&cmd.run)))
}

/// An invocation as PowerShell's text: a text as it is, a program and its arguments after `&`.
fn text(i: &Invocation) -> String {
    match i {
        Invocation::Exec(a) => format!("& {}", a.args().iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")),
        Invocation::Shell(t) => t.clone(),
        _ => "throw 'this Isoloom cannot render that command'".to_owned(),
    }
}

#[cfg(test)]
mod tests;
