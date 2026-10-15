//! The checks a parsed spec needs beyond its types: what a machine is (its `os`, its recipe, a
//! catalogue service), where its values come from (inputs, secrets, sensitive values), and what
//! its names refer to (ports, overrides, the message's placeholders). The checks on the parts
//! this format keeps from the earlier one (networks, addresses, reach, volumes, checks) run on
//! the lowered spec ([`crate::validate`]).

mod messages;
mod overrides;
mod values;

use super::Spec;
use super::app::{Recipe, Step, has_extension};
use super::machine::{Kind, Machine};
use super::problem::{Code, FieldPath, Problem, Problems};
use super::process::{HealthProbe, PortRef};

/// The spec's problems, placed, in file order.
#[must_use]
pub fn check(spec: &Spec) -> Problems {
    let mut out = Problems::new();
    for (name, m) in &spec.machines {
        let at = FieldPath::new("machines").key(name);
        what_it_is(&at, m, &mut out);
        process(&at, m, &mut out);
        values::machine(spec, &at, m, &mut out);
    }
    overrides::check(spec, &mut out);
    messages::check(spec, &mut out);
    let mut placed: Problems = out.into_iter().map(|p| spec.spans.place(p)).collect();
    placed.sort_by_key(|p| p.span.map(|s| (s.line, s.column)));
    placed
}

/// The machine's `os`, its recipe and its catalogue service.
fn what_it_is(at: &FieldPath, m: &Machine, out: &mut Problems) {
    let container_only = matches!(&m.kind, Kind::App(a) if a.container.is_some() && a.steps.is_empty());
    let supplied = m.access && matches!(m.kind, Kind::Bare);
    match &m.kind {
        Kind::App(a) if !a.steps.is_empty() && m.os.is_none() => {
            out.push(Problem::new(Code::MissingField, at.key("os"), "steps install onto an OS").fix("`os: debian@12`"));
        }
        _ if m.os.is_none() && !container_only && !supplied => {
            out.push(Problem::new(Code::MissingField, at.key("os"), "a machine says what it is: its OS, plus a service or an app").fix("`os: debian@12`"));
        }
        _ => {}
    }
    if let Kind::Service(s) = &m.kind {
        for (i, service) in s.services.iter().enumerate() {
            let path = if s.services.len() == 1 {
                at.key("service")
            } else {
                at.key("service").index(i)
            };
            let reason = format!("no catalogue entry `{}` yet: the catalogue arrives in Phase 4", service.name());
            out.push(
                Problem::new(Code::UnknownReference, path, reason)
                    .fix("until then, a custom app: `app: { image: <ref> }` or `app: { steps: [...] }` with `os`"),
            );
        }
        if m.command.is_some() {
            out.push(Problem::new(Code::Conflict, at.key("command"), "a catalogue service runs its own process").fix("leave `command` out, or use `app:`"));
        }
    }
    if let Some(a) = m.app() {
        for (i, step) in a.steps.iter().enumerate() {
            step_file(&at.key("app").key("steps").index(i), m, step, out);
        }
        if let Some(Recipe::Appliance(_)) = &a.container
            && !a.steps.is_empty()
        {
            out.push(Problem::new(Code::Conflict, at.key("app").key("steps"), "an appliance has no VM form for steps to set up").fix("leave `steps` out"));
        }
    }
}

/// A step's file has the kind the step and the OS need.
fn step_file(at: &FieldPath, m: &Machine, step: &Step, out: &mut Problems) {
    let windows = m.os.is_some_and(super::os::OsRef::is_windows);
    match step {
        Step::Run { script, .. } if windows && !has_extension(script, "ps1") => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("run"),
                format!("`{script}`: a Windows machine runs PowerShell scripts (`.ps1`)"),
            ));
        }
        Step::Run { script, .. } if !windows && !has_extension(script, "sh") => {
            out.push(
                Problem::new(Code::InvalidValue, at.key("run"), format!("`{script}`: a step runs a shell script (`.sh`)"))
                    .fix("a playbook is `{ playbook: <file>.yml }`"),
            );
        }
        Step::Playbook(p) if windows => {
            out.push(
                Problem::new(
                    Code::InvalidValue,
                    at.key("playbook"),
                    format!("`{p}`: a playbook runs on a Linux machine; a Windows machine's steps are PowerShell scripts"),
                )
                .fix("`{ run: <script>.ps1 }`"),
            );
        }
        Step::Playbook(p) if !has_extension(p, "yml") && !has_extension(p, "yaml") => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("playbook"),
                format!("`{p}`: a playbook is a `.yml` file"),
            ));
        }
        Step::Copy(c) if !c.to.starts_with('/') && !crate::model::is_drive_path(&c.to) => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("copy").key("to"),
                format!("`{}`: copy to an absolute path", c.to),
            ));
        }
        Step::Packages(p) if windows || p.is_empty() => {
            let why = if windows {
                "Windows has no package installer Isoloom drives: install from a `.ps1` step"
            } else {
                "name the packages to install"
            };
            out.push(Problem::new(Code::InvalidValue, at.key("packages"), why));
        }
        Step::Reboot if windows => {
            out.push(Problem::new(
                Code::InvalidValue,
                at.clone(),
                "a Windows machine restarts from its own script, not a `reboot` step",
            ));
        }
        _ => {}
    }
}

/// Its `setup` scripts, its ports and its healthcheck.
fn process(at: &FieldPath, m: &Machine, out: &mut Problems) {
    let windows = m.os.is_some_and(super::os::OsRef::is_windows);
    for (i, script) in m.setup.iter().enumerate() {
        let ok = if windows { has_extension(script, "ps1") } else { has_extension(script, "sh") };
        if !ok {
            let want = if windows { "`.ps1` on Windows" } else { "a `.sh` script" };
            out.push(Problem::new(
                Code::InvalidValue,
                at.key("setup").index(i),
                format!("`{script}`: a setup script is {want}"),
            ));
        }
    }
    let service = matches!(m.kind, Kind::Service(_));
    for (name, p) in &m.ports {
        if p.port.is_none() && !service {
            out.push(
                Problem::new(Code::MissingField, at.key("ports").key(name.as_str()).key("port"), "a port says its number")
                    .fix(format!("`{name}: {{ port: 8080 }}`")),
            );
        }
    }
    if let Some(HealthProbe::Http(page)) = m.healthcheck.as_ref().map(|h| &h.probe)
        && let PortRef::Name(n) = &page.port
        && !m.ports.contains_key(n)
    {
        out.push(port_reference(&at.key("healthcheck").key("http").key("port"), n.as_str(), m));
    }
}

/// A port name that names none of the machine's ports.
fn port_reference(at: &FieldPath, name: &str, m: &Machine) -> Problem {
    let known: Vec<String> = m.ports.keys().map(|k| format!("`{k}`")).collect();
    let p = Problem::new(Code::UnknownReference, at.clone(), format!("the machine has no port named `{name}`"));
    if known.is_empty() {
        p.fix("declare it under `ports:`")
    } else {
        p.fix(format!("its ports: {}", known.join(", ")))
    }
}

#[cfg(test)]
mod tests;
