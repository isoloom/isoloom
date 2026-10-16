//! What every backend's run manifest is built from: its header, steps (a native command or a
//! driver action, with their policies), tools, the lab's networks, a machine's addresses and
//! ports, the outputs of its published ports, and the checks a runner reports with the time
//! its step may take. Pure functions over the plan, shared so each target says them once.
//! Compose inside a VM is [`guest`]: what both backends of `docker-vm` share.

pub mod guest;

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use isoloom_manifest::{
    Action, Arch, Argv, Check, CheckKind, Cidr, Delivery, EnvName, Error as ManifestError, Generator, GeneratorName, HostArgv, Lab, LabName, Machine, Name,
    Network, NonEmpty, Output, OutputRole, OutputSource, Param, Part, Placeholder, Port, PortKind, PortName, Program, Protocol, Publish, PublishMode, Role,
    Run, SetBy, SnakeName, Step, StepId, StepSelection, Template, Tool, ToolName, Version, When,
};
use isoloom_plan::checks::{Probe, Resolved};
use isoloom_plan::{Environment, MachinePlan, PublishedPort};
use isoloom_spec::model;

/// What a check runner's step may add to its checks' waits: the probes' own time (curl and nc
/// cap a probe at 5 to 10 s) per check, and the runner's start.
const PER_CHECK_S: u32 = 15;
const START_S: u32 = 60;

/// Isoloom, at this build's version: who wrote the manifest.
///
/// # Errors
///
/// Fails when the workspace's version isn't `major.minor.patch` (a bug of the build).
pub fn generator() -> Result<Generator, ManifestError> {
    Ok(Generator {
        name: GeneratorName::Isoloom,
        version: Version::parse(env!("CARGO_PKG_VERSION"))?,
    })
}

/// The lab `env` plans: its name and instance, its message left to the caller.
///
/// # Errors
///
/// Fails when the environment's name or instance isn't one the manifest takes (a bug: the spec
/// checks both).
pub fn lab(env: &Environment) -> Result<Lab, ManifestError> {
    Ok(Lab {
        name: LabName::parse(&env.name)?,
        instance: env.instance.map(isoloom_manifest::Instance::new).transpose()?,
        message: None,
    })
}

/// A bare tool name, as a step's program.
///
/// # Errors
///
/// Fails on a name the manifest refuses as a program (a shell, a path).
pub fn program(name: &str) -> Result<Program, ManifestError> {
    Program::parse(name)
}

/// `program` with `args`, the same on every host.
///
/// # Errors
///
/// Fails as [`program`] does.
pub fn argv(name: &str, args: &[&str]) -> Result<HostArgv, ManifestError> {
    Ok(HostArgv::Every(Argv::new(program(name)?, args.iter().map(|a| Template::text(a)).collect())))
}

/// A step running `argv` for at most `timeout_s` seconds.
///
/// # Errors
///
/// Fails when `id` isn't a step id.
pub fn run(id: &str, argv: HostArgv, timeout_s: u32) -> Result<Step, ManifestError> {
    let mut s = Step::argv(StepId::parse(id)?, argv);
    s.timeout_s = NonZeroU32::new(timeout_s);
    Ok(s)
}

/// A step running the driver's `action`.
///
/// # Errors
///
/// Fails when `id` isn't a step id.
pub fn act(id: &str, action: Action) -> Result<Step, ManifestError> {
    Ok(Step::action(StepId::parse(id)?, action))
}

/// Driver action `action` with its arguments `with` (names, then templates).
///
/// # Errors
///
/// Fails when a name isn't one an argument can have.
pub fn action_with<'a>(action: Action, with: impl IntoIterator<Item = (&'a str, Template)>) -> Result<Run, ManifestError> {
    Ok(Run::Action {
        action,
        with: with
            .into_iter()
            .map(|(k, v)| Ok((SnakeName::parse(k)?, v)))
            .collect::<Result<BTreeMap<_, _>, ManifestError>>()?,
    })
}

/// `ISOLOOM_DERIVED`, the switch of the checks derived from the spec (`0` skips them), delivered
/// to the check runners' `steps`; `None` when there is none.
///
/// # Errors
///
/// Fails on a name the manifest refuses (a bug: it is fixed).
pub fn derived_switch(steps: Vec<StepId>) -> Result<Option<Param>, ManifestError> {
    let Ok(steps) = NonEmpty::new(steps, "the check steps") else {
        return Ok(None);
    };
    Ok(Some(Param {
        name: EnvName::parse(DERIVED)?,
        description: "0: the checks derived from the spec (reachability, no internet) are skipped.".into(),
        default: Some("1".into()),
        values: vec!["0".into(), "1".into()],
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: NonEmpty::one(Delivery::Env {
            var: EnvName::parse(DERIVED)?,
            steps: StepSelection::Only(steps),
        }),
    }))
}

/// The derived checks' switch, as a runner reads it.
pub const DERIVED: &str = "ISOLOOM_DERIVED";

/// `s` as a clean-up: it runs whatever happened before it, and its failure is only logged.
#[must_use]
pub fn cleanup(mut s: Step) -> Step {
    s.when = When::Always;
    s.failure = isoloom_manifest::Failure::Tolerate;
    s
}

/// `s`, its failure only logged.
#[must_use]
pub fn tolerated(mut s: Step) -> Step {
    s.failure = isoloom_manifest::Failure::Tolerate;
    s
}

/// `s`, saying `note` when it starts.
#[must_use]
pub fn noted(mut s: Step, note: &str) -> Step {
    s.note = Some(note.to_owned());
    s
}

/// A tool named `name`, probed with `probe`, on every host and provider.
///
/// # Errors
///
/// Fails when `name` isn't a tool name.
pub fn tool(name: &str, probe: HostArgv) -> Result<Tool, ManifestError> {
    Ok(Tool {
        name: ToolName::parse(name)?,
        probe,
        min_version: None,
        container: None,
        container_hosts: BTreeSet::new(),
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
    })
}

/// The lab's networks, each with its name where the target creates it (`native`).
///
/// # Errors
///
/// Fails on a network name or block the manifest refuses (a bug: the spec checks both).
pub fn networks(env: &Environment, native: impl Fn(&str) -> Template) -> Result<Vec<Network>, ManifestError> {
    env.networks
        .iter()
        .map(|(name, n)| {
            Ok(Network {
                name: Name::parse(name)?,
                cidr: Cidr::parse(&n.cidr)?,
                internet: n.internet,
                native: Some(native(name)),
            })
        })
        .collect()
}

/// A CPU architecture as the manifest names it.
#[must_use]
pub fn arch(a: model::Arch) -> Arch {
    match a {
        model::Arch::Amd64 => Arch::Amd64,
        model::Arch::Arm64 => Arch::Arm64,
    }
}

fn protocol(p: model::Protocol) -> Protocol {
    if p.is_tcp() { Protocol::Tcp } else { Protocol::Udp }
}

/// A machine's address on each network: IPv4 where it has one, else IPv6.
///
/// # Errors
///
/// Fails on a network name the manifest refuses.
pub fn addresses(mp: &MachinePlan) -> Result<BTreeMap<Name, IpAddr>, ManifestError> {
    let v4 = mp.addresses.iter().map(|(n, a)| (n, IpAddr::V4(*a)));
    let v6 = mp
        .addresses6
        .iter()
        .filter(|(n, _)| !mp.addresses.contains_key(*n))
        .map(|(n, a)| (n, IpAddr::V6(*a)));
    v4.chain(v6).map(|(n, a)| Ok((Name::parse(n)?, a))).collect()
}

/// A port of the lab's, named `label` (the spec's name), published as `published` plans it,
/// listening on `address` on the host.
///
/// # Errors
///
/// Fails on a name the manifest refuses, or port 0.
pub fn port(label: &str, p: &model::Port, published: &[PublishedPort], address: &Template) -> Result<Port, ManifestError> {
    let publish = published.iter().find(|x| x.name == label).map(|x| Publish {
        mode: if x.fixed { PublishMode::Fixed } else { PublishMode::Ephemeral },
        preferred: NonZeroU16::new(x.publish),
        address: Some(address.clone()),
    });
    Ok(Port {
        name: PortName::parse(label)?,
        port: NonZeroU16::new(p.port).ok_or(ManifestError::OutOfRange {
            what: "a port",
            value: 0,
            min: 1,
            max: 65535,
        })?,
        kind: PortKind::from_parts(protocol(p.protocol), p.http, p.tls)?,
        publish,
    })
}

/// A bare machine with the given `name` and `role`, every other field empty: a caller fills its
/// resources, addresses, native name and ports.
///
/// # Errors
///
/// Fails on a name the manifest refuses.
pub fn machine(name: &str, role: Role) -> Result<Machine, ManifestError> {
    Ok(Machine {
        name: Name::parse(name)?,
        role,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses: std::collections::BTreeMap::new(),
        keep_running: false,
        native: None,
        volumes: Vec::new(),
        ports: Vec::new(),
    })
}

/// A Terraform state output `name` (role `role`), read by the runner as `${outputs.<name>}`.
///
/// # Errors
///
/// Fails on a name the manifest refuses.
pub fn terraform_output(name: &str, role: OutputRole, description: &str) -> Result<Output, ManifestError> {
    Ok(Output {
        name: SnakeName::parse(name)?,
        description: Some(description.to_owned()),
        role,
        sensitive: false,
        format: None,
        from: OutputSource::TerraformOutput {
            name: isoloom_manifest::Identifier::parse(name)?,
            default: None,
        },
    })
}

/// An output per published port of the lab's machines and tools: an HTTP port is an entry (the
/// Open button), the others information; each shown at the host's loopback.
///
/// # Errors
///
/// Fails on an output name the manifest refuses.
pub fn outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    outputs_at(machines, &Template::text("127.0.0.1"))
}

/// [`outputs`], each shown at `host` (a server lab's router, `${outputs.ip}`).
///
/// # Errors
///
/// Fails on an output name the manifest refuses.
pub fn outputs_at(machines: &[Machine], host: &Template) -> Result<Vec<Output>, ManifestError> {
    outputs_with(machines, |_| host.clone())
}

/// [`outputs`], each shown at the host the driver reports for its machine (`${host.<machine>}`):
/// the loopback with the forwarded port, or, where the provider forwards nothing (ESXi), the
/// machine's own address with its own port.
///
/// # Errors
///
/// Fails on an output name the manifest refuses.
pub fn outputs_on_their_hosts(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    outputs_with(machines, |m| Template::from(Placeholder::Host(m.name.clone())))
}

/// [`outputs`], machine `m`'s shown at `host(m)`.
fn outputs_with(machines: &[Machine], host: impl Fn(&Machine) -> Template) -> Result<Vec<Output>, ManifestError> {
    let shown = machines.iter().filter(|m| matches!(m.role, Role::Target | Role::Access | Role::Tool));
    shown
        .flat_map(|m| m.ports.iter().filter(|p| p.publish.is_some()).map(move |p| (m, p)))
        .map(|(m, p)| output(m, p, &host(m)))
        .collect()
}

fn output(m: &Machine, p: &Port, host: &Template) -> Result<Output, ManifestError> {
    let snake: String = format!("{}_{}", m.name, p.name)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let (role, scheme, after) = match p.kind {
        PortKind::Https => (OutputRole::Entry, "https://", "/"),
        PortKind::Http => (OutputRole::Entry, "http://", "/"),
        PortKind::Tcp | PortKind::Udp | PortKind::Tls => (OutputRole::Info, "", ""),
    };
    let port = if p.kind == PortKind::Udp {
        format!("{}/udp", p.port)
    } else {
        p.port.to_string()
    };
    let what = format!("{}'s {} (port {port})", m.name, p.name);
    Ok(Output {
        name: SnakeName::parse(&snake)?,
        description: Some(format!("{what}.")),
        role,
        sensitive: false,
        format: Some(Template::from_parts(
            std::iter::once(Part::Text(scheme.into())).chain(host.parts().iter().cloned()).chain([
                Part::Text(":".into()),
                Part::Placeholder(Placeholder::Value),
                Part::Text(after.into()),
            ]),
        )),
        from: OutputSource::PublishedPort {
            machine: m.name.clone(),
            port: p.name.clone(),
        },
    })
}

/// A planned check, as its runner reports it.
#[must_use]
pub fn check(c: &Resolved) -> Check {
    let kind = match c.probe {
        Probe::Http { .. } => CheckKind::Http,
        Probe::Tcp { .. } => CheckKind::Tcp,
        Probe::Exec { .. } => CheckKind::Exec,
        Probe::Script { .. } => CheckKind::Script,
        Probe::Playbook { .. } => CheckKind::Playbook,
    };
    Check {
        name: c.name.clone(),
        kind,
        wait_s: c.wait,
        derived: c.derived,
    }
}

/// The time limit of a runner of `checks`, the same on every target: their waits, plus 15 s
/// each, plus 60 s. A runner never stops before its slowest check could pass; booting what the
/// runner runs on is not check time (a setup step's).
#[must_use]
pub fn timeout_s(checks: &[&Resolved]) -> u32 {
    let waits: u32 = checks.iter().map(|c| c.wait).sum();
    let count = u32::try_from(checks.len()).unwrap_or(u32::MAX);
    waits.saturating_add(count.saturating_mul(PER_CHECK_S)).saturating_add(START_S)
}

#[cfg(test)]
mod tests;
