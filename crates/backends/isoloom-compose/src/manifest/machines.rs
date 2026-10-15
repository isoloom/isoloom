//! Every service of the Compose file as a manifest machine, with the role this backend gave it
//! when it added the service (the names below are the ones it builds them with), and the outputs
//! of the ports they publish.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::num::NonZeroU16;

use isoloom_manifest::{
    Arch, GuestPath, Machine, ManifestError, Name, Native, Output, OutputRole, OutputSource, Part, Placeholder, Port, Protocol, Publish, PublishMode,
    Resources, Role, SnakeName, Template, Volume,
};
use isoloom_plan::checks::{self, Position};
use isoloom_plan::{Environment, Lifetime, MachinePlan, PublishedPort};
use isoloom_render::{router, trunks};
use isoloom_spec::model;

use isoloom_render::container::init_names;

use super::in_project;
use crate::error::{Error, Result};
use crate::file::ComposeFile;
use crate::names::{STAND_IN, check_runner_service, probe_service, routes_service, tool_service};
use crate::storage::FOLDERS_JOB;

/// A service of the Compose file, told apart.
pub(super) struct Member {
    /// The service.
    pub(super) name: String,
    /// What it is to the run.
    pub(super) role: Role,
    /// The machine it belongs to.
    pub(super) of: Option<String>,
}

/// Every service of `file`, in its order, with its role; a service this backend doesn't know it
/// added is refused.
pub(super) fn classify(env: &Environment, file: &ComposeFile) -> Result<Vec<Member>> {
    file.services
        .keys()
        .map(|name| {
            let (role, of) = role_of(env, file, name).ok_or_else(|| bug_unknown(name))?;
            Ok(Member { name: name.clone(), role, of })
        })
        .collect()
}

/// A service this backend doesn't know it added: a bug, which the manifest's types name.
fn bug_unknown(name: &str) -> Error {
    Error::from(ManifestError::UnknownMachine {
        at: "the Compose file".into(),
        machine: name.into(),
    })
}

fn role_of(env: &Environment, file: &ComposeFile, name: &str) -> Option<(Role, Option<String>)> {
    let spec = env.spec();
    if let Some(m) = spec.machines.get(name) {
        return Some((if m.access { Role::Access } else { Role::Target }, None));
    }
    let owned = spec.machines.iter().find_map(|(m, machine)| {
        if init_names(m, machine).iter().any(|j| j == name) {
            Some((Role::Job, Some(m.clone())))
        } else {
            (name == routes_service(m)).then(|| (Role::Infra, Some(m.clone())))
        }
    });
    owned.or_else(|| isoloom_own(env, name)).or_else(|| runner(env, file, name))
}

/// Isoloom's own services: the stand-in and its routes, tools, the router, switches, the
/// folders job and the probes.
fn isoloom_own(env: &Environment, name: &str) -> Option<(Role, Option<String>)> {
    let spec = env.spec();
    if name == STAND_IN {
        return Some((Role::Access, None));
    }
    if name == routes_service(STAND_IN) {
        return Some((Role::Infra, Some(STAND_IN.into())));
    }
    if spec.tools.keys().any(|t| tool_service(t) == name) {
        return Some((Role::Tool, None));
    }
    let switch = || trunks::trunks(spec).iter().any(|t| trunks::switch_name(&t.lan) == name);
    let probe = || [model::Arch::Amd64, model::Arch::Arm64].into_iter().any(|a| probe_service(a) == name);
    (name == router::NAME || name == FOLDERS_JOB || switch() || probe()).then_some((Role::Infra, None))
}

/// A check runner, belonging to the machine whose namespace it runs in.
fn runner(env: &Environment, file: &ComposeFile, name: &str) -> Option<(Role, Option<String>)> {
    let spec = env.spec();
    let default = checks::default_position(spec);
    let (pos, _) = checks::by_position(spec, &env.checks)
        .into_iter()
        .find(|(pos, _)| check_runner_service(pos, &default) == name)?;
    let of = match pos {
        Position::Machine(m) if file.services.contains_key(&m) => Some(m),
        Position::Machine(_) => Some(STAND_IN.to_owned()),
        Position::Networks => None,
    };
    Some((Role::Check, of))
}

/// Each member as a machine: the lab's own with their architecture, size, addresses, volumes
/// and ports; Isoloom's with what they have of those.
pub(super) fn machines(env: &Environment, file: &ComposeFile, members: &[Member]) -> Result<Vec<Machine>> {
    members.iter().map(|m| machine(env, file, m)).collect()
}

fn machine(env: &Environment, file: &ComposeFile, member: &Member) -> Result<Machine> {
    let name = Name::parse(&member.name)?;
    let mut out = Machine {
        name: name.clone(),
        role: member.role,
        of: member.of.as_deref().map(Name::parse).transpose()?,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses: BTreeMap::new(),
        keep_running: false,
        native: Some(Native::Compose { service: name }),
        volumes: Vec::new(),
        ports: Vec::new(),
    };
    let spec = env.spec();
    if let (Some(mp), Some(m)) = (env.machines.get(&member.name), spec.machines.get(&member.name)) {
        out.arch = Some(arch(mp.arch));
        out.resources = (mp.size.cpus.is_some() || mp.size.memory_mb.is_some()).then_some(Resources {
            cpus: mp.size.cpus,
            memory_mb: mp.size.memory_mb,
        });
        out.addresses = addresses(mp)?;
        out.volumes = volumes(mp)?;
        out.ports = m.ports.iter().map(|p| port(&m.port_label(p), p, &mp.ports)).collect::<Result<_, _>>()?;
    } else if member.name == STAND_IN {
        // It stands in at the addresses of the machine its hostname names.
        let stood_for = file.services.get(STAND_IN).and_then(|s| s.hostname.as_deref());
        if let Some(mp) = stood_for.and_then(|h| env.machines.get(h)) {
            out.addresses = addresses(mp)?;
        }
    } else if member.name == router::NAME {
        let router = env.router.iter().flatten().map(|(n, a)| Ok((Name::parse(n)?, IpAddr::V4(*a))));
        out.addresses = router.collect::<Result<_>>()?;
    } else if let Some(tool) = spec.tools.iter().find(|(t, _)| tool_service(t) == member.name).map(|(_, t)| t) {
        out.ports = tool_port(tool)?.into_iter().collect();
    }
    Ok(out)
}

fn arch(a: model::Arch) -> Arch {
    match a {
        model::Arch::Amd64 => Arch::Amd64,
        model::Arch::Arm64 => Arch::Arm64,
    }
}

fn protocol(p: model::Protocol) -> Protocol {
    if p.is_tcp() { Protocol::Tcp } else { Protocol::Udp }
}

/// Its address on each network: IPv4 where it has one, else IPv6.
fn addresses(mp: &MachinePlan) -> Result<BTreeMap<Name, IpAddr>> {
    let v4 = mp.addresses.iter().map(|(n, a)| (n, IpAddr::V4(*a)));
    let v6 = mp
        .addresses6
        .iter()
        .filter(|(n, _)| !mp.addresses.contains_key(*n))
        .map(|(n, a)| (n, IpAddr::V6(*a)));
    v4.chain(v6).map(|(n, a)| Ok((Name::parse(n)?, a))).collect()
}

/// Its named volumes and data disks, by their names in the spec.
fn volumes(mp: &MachinePlan) -> Result<Vec<Volume>> {
    let named = mp.volumes.iter().map(|v| {
        let native = match &v.lifetime {
            Lifetime::External { name } => Template::text(name),
            Lifetime::Environment | Lifetime::Persistent => in_project(&v.key),
        };
        (v.name.as_str(), v.path.as_str(), native)
    });
    let disks = mp.disks.iter().filter_map(|d| Some((d.name.as_str(), d.mount.as_deref()?, in_project(&d.key))));
    named
        .chain(disks)
        .map(|(name, path, native)| {
            Ok(Volume {
                name: Name::parse(name)?,
                path: GuestPath::parse(path)?,
                native: Some(native),
            })
        })
        .collect()
}

/// Where a published port listens: `ISOLOOM_PUBLISH_ADDRESS`.
fn publish_address() -> Result<Template> {
    let name = isoloom_manifest::EnvName::parse(super::slots::PUBLISH_ADDRESS)?;
    Ok(Template::from(Placeholder::Param(name)))
}

/// A port of the lab's, named `label` (the spec's name).
fn port(label: &str, p: &model::Port, published: &[PublishedPort]) -> Result<Port> {
    let publish = published
        .iter()
        .find(|x| x.name == label)
        .map(|x| {
            Ok::<_, Error>(Publish {
                mode: if x.fixed { PublishMode::Fixed } else { PublishMode::Ephemeral },
                preferred: NonZeroU16::new(x.publish),
                address: Some(publish_address()?),
            })
        })
        .transpose()?;
    Ok(Port {
        name: Name::parse(label)?,
        port: NonZeroU16::new(p.port).ok_or_else(|| {
            Error::from(ManifestError::OutOfRange {
                what: "a port",
                value: 0,
                min: 1,
                max: 65535,
            })
        })?,
        protocol: protocol(p.protocol),
        http: p.http,
        tls: p.tls,
        publish,
    })
}

/// The name of a tool's port (its web UI).
const TOOL_PORT: &str = "web";

/// A tool's port, always published on its own `publish` number.
fn tool_port(tool: &model::Tool) -> Result<Option<Port>> {
    let (Some(port), Some(publish)) = (tool.port.and_then(NonZeroU16::new), tool.publish) else {
        return Ok(None);
    };
    Ok(Some(Port {
        name: Name::parse(TOOL_PORT)?,
        port,
        protocol: Protocol::Tcp,
        http: false,
        tls: false,
        publish: Some(Publish {
            mode: PublishMode::Fixed,
            preferred: NonZeroU16::new(publish),
            address: Some(publish_address()?),
        }),
    }))
}

/// An output per published port of the lab's machines and tools: an HTTP port is an entry (the
/// Open button), the others information; each shown at the host's loopback.
pub(super) fn outputs(machines: &[Machine]) -> Result<Vec<Output>> {
    let shown = machines.iter().filter(|m| matches!(m.role, Role::Target | Role::Access | Role::Tool));
    shown
        .flat_map(|m| m.ports.iter().filter(|p| p.publish.is_some()).map(move |p| (m, p)))
        .map(|(m, p)| output(m, p))
        .collect()
}

fn output(m: &Machine, p: &Port) -> Result<Output> {
    let snake: String = format!("{}_{}", m.name, p.name)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let (role, before, after) = match (p.http, p.tls) {
        (true, true) => (OutputRole::Entry, "https://127.0.0.1:", "/"),
        (true, false) => (OutputRole::Entry, "http://127.0.0.1:", "/"),
        (false, _) => (OutputRole::Info, "127.0.0.1:", ""),
    };
    let port = if p.protocol == Protocol::Udp {
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
        format: Some(Template::from_parts([
            Part::Text(before.into()),
            Part::Placeholder(Placeholder::Value),
            Part::Text(after.into()),
        ])),
        from: OutputSource::PublishedPort {
            machine: m.name.clone(),
            port: p.name.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use isoloom_manifest::ManifestError;
    use isoloom_spec::model::Target;

    use crate::error::Error;
    use crate::file::Service;
    use crate::testing::environment;

    #[test]
    fn a_service_this_backend_did_not_add_is_refused() {
        let env = environment(
            "version: 1
name: t
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lan: 10 }, app: { image: nginx } }
",
        );
        let mut file = crate::render::compose_file(Target::Docker, &env).expect("renders").file;
        assert!(super::classify(&env, &file).is_ok());
        file.services.insert("mystery".into(), Service::default());
        let err = super::classify(&env, &file).err();
        assert!(
            matches!(&err, Some(Error::Manifest { source: ManifestError::UnknownMachine { machine, .. } }) if machine == "mystery"),
            "{err:?}"
        );
    }
}
