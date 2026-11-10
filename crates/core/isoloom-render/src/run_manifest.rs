//! What every backend's run manifest is built from: its header, steps (a native command or a
//! driver action, with their policies), tools, the environment's networks, a machine's addresses and
//! ports, the outputs of its published ports, and the checks a runner reports with the time
//! its step may take. Pure functions over the plan, shared so each target says them once.
//! Compose inside a VM is [`guest`]: what both backends of `docker-vm` share.

pub mod guest;

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use isoloom_manifest::{
    Action, Arch, Argv, Check, CheckKind, Delivery, EnvName, EnvironmentName, EnvironmentRef, Error as ManifestError, Generator, GeneratorName, HostArgv,
    HostOs, Machine, Name, Network, NonEmpty, Output, OutputRole, OutputSource, Param, Part, Placeholder, Port, PortKind, PortName, Program, Protocol, Publish,
    PublishMode, Role, Run, SetBy, SnakeName, Step, StepId, StepSelection, Template, Tool, ToolName, Version, When,
};
use isoloom_plan::checks::{Probe, Resolved};
use isoloom_plan::{Environment, MachinePlan, MachineShape, PublishedPort};
use isoloom_spec::model;

/// Where the runner reads secret `source` when the embedder gives no value: its store and what
/// the store is asked for, as the spec writes it. An input's file has none: the input's own slot
/// carries its value.
#[must_use]
fn secret_source(source: &model::SecretSource) -> Option<isoloom_manifest::SecretSource> {
    let store = match source {
        model::SecretSource::Env { .. } => isoloom_manifest::Store::Env,
        model::SecretSource::File { .. } => isoloom_manifest::Store::File,
        model::SecretSource::Vault { .. } => isoloom_manifest::Store::Vault,
        model::SecretSource::Sops { .. } => isoloom_manifest::Store::Sops,
        model::SecretSource::Input { .. } => return None,
    };
    Some(isoloom_manifest::SecretSource {
        store,
        reference: source.reference().to_owned(),
    })
}

/// An environment machine's role in a run manifest: `access` for the machine the user lands on,
/// `member` for every other.
#[must_use]
pub fn role(mp: &MachinePlan) -> Role {
    match mp.role {
        isoloom_plan::MachineRole::Access => Role::Access,
        isoloom_plan::MachineRole::Member => Role::Member,
    }
}

/// A secret the runner reads from its store, and the machines of one shape that get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldSecret<'a> {
    /// The secret's name.
    pub name: &'a str,
    /// Where the runner reads its value.
    pub source: isoloom_manifest::SecretSource,
    /// The machines of the shape that get it, in spec order.
    pub holders: Vec<&'a str>,
}

/// Each secret of `env` the runner reads from a store (`env`, `file`, `vault`, `sops`: an
/// input's file is the input's own slot) that machines of `shape` get, with those machines: what
/// a run manifest's secret slots are made of. A secret no such machine gets is left out.
#[must_use]
pub fn held_secrets(env: &Environment, shape: MachineShape) -> Vec<HeldSecret<'_>> {
    env.secrets
        .iter()
        .filter_map(|(name, source)| {
            let source = secret_source(source)?;
            let holders: Vec<&str> = env
                .machines
                .iter()
                .filter(|(_, m)| m.shape == shape && m.secrets.iter().any(|s| &s.name == name))
                .map(|(n, _)| n.as_str())
                .collect();
            (!holders.is_empty()).then_some(HeldSecret { name, source, holders })
        })
        .collect()
}

/// What generated files and messages call the program that executes a run manifest: no
/// particular one, the contract it follows. The one phrase every target says it with.
pub const RUNNER: &str = "the runner (run.json)";

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

/// The environment `env` plans: its name and instance, its message left to the caller.
///
/// # Errors
///
/// Fails when the environment's name or instance isn't one the manifest takes (a bug: the spec
/// checks both).
pub fn environment(env: &Environment) -> Result<EnvironmentRef, ManifestError> {
    Ok(EnvironmentRef {
        name: EnvironmentName::parse(&env.name)?,
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

/// `project.archive` of the project's folder `root` (from the manifest's), with Isoloom's `outputs`
/// the guest reads (`with.outputs`, left out when there are none).
///
/// # Errors
///
/// None in practice: the step's id and keys are fixed.
pub fn project_archive(root: &str, outputs: &[&str]) -> Result<Step, ManifestError> {
    let outputs = (!outputs.is_empty()).then(|| ("outputs", Template::text(&outputs.join(" "))));
    let mut s = Step::action(StepId::parse("archive")?, Action::ProjectArchive);
    s.run = action_with(Action::ProjectArchive, std::iter::once(("root", Template::text(root))).chain(outputs))?;
    s.note = Some("Archiving the project…".into());
    Ok(s)
}

/// `ISOLOOM_DERIVED` ([`isoloom_manifest::contract::DERIVED`]), the switch of the checks derived
/// from the spec (`0` skips them), delivered to the check runners' `steps`; `None` when there is
/// none.
///
/// # Errors
///
/// Fails on a name the manifest refuses (a bug: it is fixed).
pub fn derived_switch(steps: Vec<StepId>) -> Result<Option<Param>, ManifestError> {
    let Ok(steps) = NonEmpty::new(steps, "the check steps") else {
        return Ok(None);
    };
    let what = "0: the checks derived from the spec (reachability, no internet) are skipped.";
    embedder_param(isoloom_manifest::contract::DERIVED, what, Some("1"), &["0", "1"], StepSelection::Only(steps)).map(Some)
}

/// An optional param `name` the embedder sets (its `default` and the `values` it takes, if any
/// are listed), delivered as its own variable to `steps`.
///
/// # Errors
///
/// Fails on a name the manifest refuses (a bug: it is fixed).
pub fn embedder_param(name: &str, description: &str, default: Option<&str>, values: &[&str], steps: StepSelection) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(name)?,
        description: description.into(),
        default: default.map(String::from),
        values: values.iter().map(|v| (*v).to_owned()).collect(),
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver: NonEmpty::one(Delivery::Env {
            var: EnvName::parse(name)?,
            steps,
        }),
    })
}

/// `ISOLOOM_PUBLISH_FIXED` ([`isoloom_manifest::contract::PUBLISH_FIXED`]), delivered to every
/// step, when one of `machines` publishes a port: set, each published port takes its own
/// `publish` number on the host instead of a free one. A program running the environment (to
/// keep its own ports) may pass it whenever a port is published; it changes nothing for a port
/// already `fixed`. `None` when no port is published.
///
/// # Errors
///
/// Fails on a name the manifest refuses (a bug: it is fixed).
pub fn publish_fixed(machines: &[Machine]) -> Result<Option<Param>, ManifestError> {
    if !machines.iter().flat_map(|m| &m.ports).any(|p| p.publish.is_some()) {
        return Ok(None);
    }
    let what = "Set: each published port takes its own `publish` number on the host, instead of a free one.";
    embedder_param(isoloom_manifest::contract::PUBLISH_FIXED, what, None, &["1"], StepSelection::Every).map(Some)
}

/// Every host a run starts from: Linux, macOS and Windows.
#[must_use]
pub fn every_host() -> BTreeSet<HostOs> {
    BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows])
}

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

/// The environment's networks, each with its name where the target creates it (`native`).
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
                cidr: n.cidr.cidr().into(),
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

/// A port of the environment's, named `label` (the spec's name), published as `published` plans it,
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

/// A machine's size in its run manifest: its CPUs and memory as it runs on the target (a
/// container or a VM), each the spec leaves out the default of that form
/// (`isoloom_spec::MachineSize`).
#[must_use]
pub fn resources(mp: &MachinePlan) -> isoloom_manifest::Resources {
    let size = mp.size();
    isoloom_manifest::Resources {
        cpus: Some(size.cpus),
        memory_mb: Some(size.memory_mb),
    }
}

/// A bare machine with the given `name` and `role`, every other field empty: a caller fills its
/// resources, addresses, native name and ports. Every backend's run-manifest machine starts here.
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
        on_demand: false,
        native: None,
        volumes: Vec::new(),
        ports: Vec::new(),
        consoles: Vec::new(),
    })
}

/// An output per published port of the environment's machines and tools, named `<machine>__<port>`: an
/// HTTP port is an entry (the address a user opens first), the others information; each shown
/// at the host's loopback.
///
/// # Errors
///
/// Fails on an output name the manifest refuses.
pub fn outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    outputs_with(machines, |_| Template::text("127.0.0.1"))
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

/// [`outputs`], machine `m`'s shown at `host(m)` (a cloud machine's own public address).
///
/// # Errors
///
/// Fails on an output name the manifest refuses.
pub fn outputs_with(machines: &[Machine], host: impl Fn(&Machine) -> Template) -> Result<Vec<Output>, ManifestError> {
    let shown = machines.iter().filter(|m| matches!(m.role, Role::Member | Role::Access | Role::Tool));
    shown
        .flat_map(|m| m.ports.iter().filter(|p| p.publish.is_some()).map(move |p| (m, p)))
        .map(|(m, p)| output(m, p, &host(m)))
        .collect()
}

/// A published port's output name: its machine's name and its own in snake case, joined by
/// `__` (`web-admin`'s `ui` is `web_admin__ui`, `web`'s `admin-ui` `web__admin_ui`). A machine's
/// name has no `--`, so the first `__` ends it: no two ports share a name, and none takes a
/// Terraform output's (`ip`, `address_web`), which has single `_` only.
fn output_name(machine: &str, port: &str) -> Result<SnakeName, ManifestError> {
    SnakeName::parse(&format!("{}__{}", SnakeName::from_label(machine)?, SnakeName::from_label(port)?))
}

fn output(m: &Machine, p: &Port, host: &Template) -> Result<Output, ManifestError> {
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
        name: output_name(m.name.as_str(), p.name.as_str())?,
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
