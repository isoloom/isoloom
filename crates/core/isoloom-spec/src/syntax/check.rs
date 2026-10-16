//! The checks a parsed spec needs beyond its types: what a machine is (its `os`, its recipe, a
//! catalogue service's fields the spec alone rules on), where its values come from (inputs, secrets, sensitive values), and what
//! its names refer to (ports, overrides, the message's placeholders). The checks on the parts
//! this format keeps from the earlier one (networks, addresses, reach, volumes, checks) run on
//! the lowered spec ([`crate::validate`]).

mod messages;
mod overrides;
mod steps;
mod values;

pub use messages::{Unresolved, placeholder_problems};

use super::Spec;
use super::app::Recipe;
use super::machine::{Kind, Machine};
use super::problem::{Code, FieldPath, Problem, Problems};
use super::process::{HealthProbe, PortRef};
use super::step::has_extension;

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
    // The catalogue's entries, releases and settings are the plan's to resolve (it has the
    // catalogue): here, what the spec alone says.
    if matches!(m.kind, Kind::Service(_)) && m.command.is_some() {
        out.push(Problem::new(Code::Conflict, at.key("command"), "a catalogue service runs its own process").fix("leave `command` out, or use `app:`"));
    }
    if let Some(a) = m.app() {
        for (i, step) in a.steps.iter().enumerate() {
            steps::check(&at.key("app").key("steps").index(i), m, step, out);
        }
        if let Some(Recipe::Appliance(_)) = &a.container
            && !a.steps.is_empty()
        {
            out.push(Problem::new(Code::Conflict, at.key("app").key("steps"), "an appliance has no VM form for steps to set up").fix("leave `steps` out"));
        }
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
        if p.port.is_some() && service {
            out.push(
                Problem::new(
                    Code::Conflict,
                    at.key("ports").key(name.as_str()).key("port"),
                    "a catalogue service's entry gives its ports' numbers",
                )
                .fix("leave `port` out (publish it with `publish`), and change the number through `with:`"),
            );
        }
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
