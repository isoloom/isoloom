//! A machine's recipe resolved: its steps for its OS's family, its firewall's ports by number, its
//! services' units and foreground forms. Steps a family can't carry at all (a repository on
//! Windows, a file's owner or mode there) are refused here, so the rule is one, whichever
//! renderer asks.

use indexmap::IndexMap;
use isoloom_spec::model::Protocol;
use isoloom_spec::syntax::{self, FileState, FirewallPort, OsRef, PortName, PortProtocol, Service, Step};

use super::daemons::{Daemon, DaemonTable};
use super::family::{Family, codename};
use super::resources::{PortSpec, RepositoryKind, RepositoryPlan, Resource, RunPlan, ServicePlan, ServiceSource, Shell};

/// A recipe resolved for one machine: its phases in order (`install`, `configure`, `start`) and
/// the daemons its container form runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Recipe {
    /// The family it is resolved for.
    pub family: Family,
    /// Install: an app's steps; a catalogue entry's `install` (which its official image skips).
    pub install: Vec<Resource>,
    /// Configure: a catalogue entry's `configure`, run in every form.
    pub configure: Vec<Resource>,
    /// Start: a catalogue entry's per-start preparation (an app has none in 1.0).
    pub start: Vec<Resource>,
    /// The foreground forms of its enabled services, in recipe order: the container's daemons.
    pub daemons: Vec<Daemon>,
}

impl Recipe {
    /// Its resources, phase after phase.
    pub fn resources(&self) -> impl Iterator<Item = &Resource> {
        self.install.iter().chain(&self.configure).chain(&self.start)
    }
}

/// A step its machine's family can't carry, in any form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unportable {
    /// The step, by its index in `app.steps`.
    pub step: usize,
    /// The field under the step's kind that rules it out (`owner`), when one does.
    pub field: Option<&'static str>,
    /// Why.
    pub reason: String,
    /// What to write instead.
    pub fix: String,
}

/// The recipe of machine `m`'s steps on `os`, its services read in `daemons`.
///
/// # Errors
///
/// Every step `os`'s family can't carry ([`Unportable`]), in step order.
pub fn recipe(os: OsRef, m: &syntax::Machine, daemons: &DaemonTable) -> Result<Recipe, Vec<Unportable>> {
    let family = Family::from(os);
    let steps = m.app().map(|a| a.steps.as_slice()).unwrap_or_default();
    let unportable: Vec<Unportable> = steps.iter().enumerate().filter_map(|(i, s)| unportable(i, s, family)).collect();
    if !unportable.is_empty() {
        return Err(unportable);
    }
    let mut out = Recipe {
        family,
        install: Vec::new(),
        configure: Vec::new(),
        start: Vec::new(),
        daemons: Vec::new(),
    };
    let mut unknown = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if let Step::Service(s) = step
            && s.enabled()
        {
            out.daemons.extend(foreground(s, family, daemons));
        }
        match resource(step, os, &m.ports, daemons) {
            Some(r) => out.install.push(r),
            None => unknown.push(Unportable {
                step: i,
                field: None,
                reason: format!("this Isoloom can't resolve a `{}` step", step.kind()),
                fix: "upgrade Isoloom's plan with its spec".into(),
            }),
        }
    }
    if unknown.is_empty() { Ok(out) } else { Err(unknown) }
}

/// Why `family` can't carry step `i` at all, when it can't.
fn unportable(i: usize, step: &Step, family: Family) -> Option<Unportable> {
    if family != Family::Windows {
        return None;
    }
    let acl = |field| Unportable {
        step: i,
        field: Some(field),
        reason: format!("a Windows file's `{field}` is an ACL, which the model sets in 1.x"),
        fix: format!("leave `{field}` out, or set the ACL from a `.ps1` step"),
    };
    match step {
        Step::Repository(_) => Some(Unportable {
            step: i,
            field: None,
            reason: "Windows has no package repository Isoloom drives".into(),
            fix: "`{ run: provision/install.ps1 }`".into(),
        }),
        Step::Packages(p) if p.iter().any(|p| p.version.is_some()) => Some(Unportable {
            step: i,
            field: None,
            reason: "a Windows feature has no version".into(),
            fix: "the feature's name alone (`Web-Server`)".into(),
        }),
        Step::Copy(c) => first_set(&[("owner", c.owner.is_some()), ("mode", c.mode.is_some())]).map(acl),
        Step::Template(t) => first_set(&[("owner", t.owner.is_some()), ("group", t.group.is_some()), ("mode", t.mode.is_some())]).map(acl),
        Step::File(f) if matches!(f.state, FileState::Link(_)) => Some(Unportable {
            step: i,
            field: Some("link"),
            reason: "Windows links need privileges a set-up step doesn't have".into(),
            fix: "copy the file instead, or make the link from a `.ps1` step".into(),
        }),
        Step::File(f) => first_set(&[("owner", f.owner.is_some()), ("group", f.group.is_some()), ("mode", f.mode.is_some())]).map(acl),
        _ => None,
    }
}

/// The first of these fields that is set.
fn first_set(fields: &[(&'static str, bool)]) -> Option<&'static str> {
    fields.iter().find_map(|(field, set)| set.then_some(*field))
}

/// Step `step` resolved on `os` (`ports`: its machine's, for the firewall's names); `None` for a
/// kind the spec has and this plan doesn't (the two crates ship together, so never).
fn resource(step: &Step, os: OsRef, ports: &IndexMap<PortName, syntax::Port>, daemons: &DaemonTable) -> Option<Resource> {
    let family = Family::from(os);
    let r = match step {
        Step::Run { script, creates } => Resource::Run(RunPlan {
            script: script.clone(),
            creates: creates.clone(),
            shell: if family == Family::Windows { Shell::PowerShell } else { Shell::Posix },
        }),
        Step::Copy(c) => Resource::Copy(c.clone()),
        Step::Packages(p) => Resource::Packages(p.clone()),
        Step::Reboot => Resource::Reboot,
        Step::Playbook(p) => Resource::Playbook(p.clone()),
        Step::Repository(r) => Resource::Repository(repository(r, os)),
        Step::File(f) => Resource::File(f.clone()),
        Step::Template(t) => Resource::Template(t.clone()),
        Step::User(u) => Resource::User(u.clone()),
        Step::Group(g) => Resource::Group(g.clone()),
        Step::Service(s) => Resource::Service(service(s, family, daemons)),
        Step::Command(c) => Resource::Command(c.clone()),
        Step::Firewall(f) => Resource::Firewall(f.allow.iter().filter_map(|p| port(p, ports)).collect()),
        _ => return None,
    };
    Some(r)
}

/// A repository on `os`: apt's suite (the OS's codename) and components (`main`) filled in.
fn repository(r: &syntax::Repository, os: OsRef) -> RepositoryPlan {
    let kind = match Family::from(os) {
        Family::RedHat => RepositoryKind::Rpm,
        Family::Alpine => RepositoryKind::Apk,
        Family::Debian | Family::Windows => RepositoryKind::Apt,
    };
    let apt = kind == RepositoryKind::Apt;
    let suite = r.suite.clone().or_else(|| codename(os.os()).and_then(|c| c.parse().ok())).filter(|_| apt);
    let components = match &r.components {
        Some(c) => c.clone(),
        None if apt => "main".parse().ok().into_iter().collect(),
        None => Vec::new(),
    };
    RepositoryPlan {
        name: r.name.clone(),
        kind,
        url: r.url.clone(),
        key: r.key.clone(),
        suite,
        components,
        priority: r.priority,
    }
}

/// A service on `family`: a packaged unit's name as the family has it.
fn service(s: &Service, family: Family, daemons: &DaemonTable) -> ServicePlan {
    match s {
        Service::Defined { name, run, user, cwd, enabled } => ServicePlan {
            unit: name.clone(),
            source: ServiceSource::Defined {
                run: run.clone(),
                user: user.clone(),
                cwd: cwd.clone(),
            },
            enabled: *enabled,
        },
        _ => ServicePlan {
            unit: daemons.find(s.name(), family).map_or_else(|| s.name().clone(), |(_, f)| f.daemon.unit.clone()),
            source: ServiceSource::Package,
            enabled: s.enabled(),
        },
    }
}

/// The foreground form of service `s` on `family`: its own `run`, else the daemon table's.
fn foreground(s: &Service, family: Family, daemons: &DaemonTable) -> Option<Daemon> {
    match s {
        Service::Defined { name, run, user, .. } => Some(Daemon {
            unit: name.clone(),
            foreground: run.clone(),
            user: user.clone(),
            logs: Vec::new(),
        }),
        _ => daemons.find(s.name(), family).map(|(_, f)| f.daemon.clone()),
    }
}

/// A firewall port by number: a named one is the machine's port (the spec's checks report a
/// name the machine doesn't have).
fn port(p: &FirewallPort, ports: &IndexMap<PortName, syntax::Port>) -> Option<PortSpec> {
    match p {
        FirewallPort::Named(name) => {
            let found = ports.get(name)?;
            let protocol = if found.protocol == PortProtocol::Udp { Protocol::Udp } else { Protocol::Tcp };
            Some(PortSpec { port: found.port?, protocol })
        }
        FirewallPort::Number { port, protocol } => Some(PortSpec {
            port: *port,
            protocol: *protocol,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
