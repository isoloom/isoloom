//! The machines a Proxmox run creates (the lab's VMs and LXC containers, the router, the
//! controller), its networks, and what it gives back: the router's uplink address (`ip`, the way
//! in), the SSH user, the ready file, and each published port at the router's address.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::num::NonZeroU16;

use isoloom_manifest::{
    Arch, Cidr, Machine, ManifestError, Name, Network, OsName, Output, OutputRole, OutputSource, Part, Placeholder, Port, Protocol, Publish, PublishMode, Role,
    SnakeName, Template,
};
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::router;
use isoloom_spec::model;
use isoloom_spec::validate::Cidr as SpecCidr;

use crate::machine::controller;

/// The router, as the manifest names it.
const ROUTER: &str = "isoloom-router";
/// The controller, as the manifest names it.
const CONTROLLER: &str = "isoloom-controller";
/// The router's uplink address: every published port answers there.
const IP: &str = "ip";

/// The lab's networks (the module's VNets name themselves after the run's slot).
pub(super) fn networks(env: &Environment) -> Result<Vec<Network>, ManifestError> {
    env.networks
        .iter()
        .map(|(name, n)| {
            Ok(Network {
                name: Name::parse(name)?,
                cidr: Cidr::parse(&n.cidr)?,
                internet: n.internet,
                native: None,
            })
        })
        .collect()
}

/// The lab's machines in start order, then the router and the controller.
pub(super) fn machines(env: &Environment) -> Result<Vec<Machine>, ManifestError> {
    let mut out = Vec::new();
    for name in &env.start_order {
        let (Some(mp), Some(spec)) = (env.machines.get(name), env.spec().machines.get(name)) else {
            continue;
        };
        let mut m = machine(name, Role::Target, addresses(mp)?)?;
        m.os = mp.os.as_deref().map(OsName::parse).transpose()?;
        m.windows = mp.is_windows();
        m.arch = Some(match mp.arch {
            model::Arch::Amd64 => Arch::Amd64,
            model::Arch::Arm64 => Arch::Arm64,
        });
        m.ports = spec.ports.iter().map(|p| port(&spec.port_label(p), p)).collect::<Result<_, _>>()?;
        out.push(m);
    }
    let router_at = |net: &str| IpAddr::V4(router::address(env.spec(), net));
    out.push(machine(ROUTER, Role::Infra, by_network(env, router_at)?)?);
    if controller::needed(env) {
        let at = |net: &str| {
            let c = env.networks.get(net).and_then(|n| SpecCidr::parse(&n.cidr));
            c.map_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), |c| IpAddr::V4(c.controller()))
        };
        out.push(machine(CONTROLLER, Role::Infra, by_network(env, at)?)?);
    }
    Ok(out)
}

fn machine(name: &str, role: Role, addresses: BTreeMap<Name, IpAddr>) -> Result<Machine, ManifestError> {
    Ok(Machine {
        name: Name::parse(name)?,
        role,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses,
        keep_running: false,
        native: None,
        volumes: Vec::new(),
        ports: Vec::new(),
    })
}

/// `at` on each network of the lab.
fn by_network(env: &Environment, at: impl Fn(&str) -> IpAddr) -> Result<BTreeMap<Name, IpAddr>, ManifestError> {
    env.networks.keys().map(|n| Ok((Name::parse(n)?, at(n)))).collect()
}

/// A machine's address on each network: IPv4 where it has one, else IPv6.
fn addresses(mp: &MachinePlan) -> Result<BTreeMap<Name, IpAddr>, ManifestError> {
    let v4 = mp.addresses.iter().map(|(n, a)| (n, IpAddr::V4(*a)));
    let v6 = mp
        .addresses6
        .iter()
        .filter(|(n, _)| !mp.addresses.contains_key(*n))
        .map(|(n, a)| (n, IpAddr::V6(*a)));
    v4.chain(v6).map(|(n, a)| Ok((Name::parse(n)?, a))).collect()
}

/// A port of the spec's, named `label`: published at the router's uplink address on its own
/// `publish` number (the router forwards it).
fn port(label: &str, p: &model::Port) -> Result<Port, ManifestError> {
    let at = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    Ok(Port {
        name: Name::parse(label)?,
        port: NonZeroU16::new(p.port).ok_or(ManifestError::OutOfRange {
            what: "a port",
            value: 0,
            min: 1,
            max: 65535,
        })?,
        protocol: if p.protocol.is_tcp() { Protocol::Tcp } else { Protocol::Udp },
        http: p.http,
        tls: p.tls,
        publish: p.publish.map(|n| Publish {
            mode: PublishMode::Fixed,
            preferred: NonZeroU16::new(n),
            address: Some(at.clone()),
        }),
    })
}

/// The router's address (`ip`), the SSH user, the ready file, then each published port.
pub(super) fn outputs(env: &Environment) -> Result<Vec<Output>, ManifestError> {
    let state = |name: &str, role: OutputRole, what: &str| -> Result<Output, ManifestError> {
        Ok(Output {
            name: SnakeName::parse(name)?,
            description: Some(what.into()),
            role,
            sensitive: false,
            format: None,
            from: OutputSource::TerraformOutput {
                name: isoloom_manifest::Identifier::parse(name)?,
                default: None,
            },
        })
    };
    let mut out = vec![
        state(
            IP,
            OutputRole::Address,
            "The router's uplink address: the published ports answer there, and SSH reaches the machines through it.",
        )?,
        state(
            "ssh_user",
            OutputRole::Info,
            "The user every Linux VM and the router let in with the runner's key.",
        )?,
        state(
            "ready_file",
            OutputRole::Info,
            "Where each Linux machine's set-up says how far it got (isoloom-ready/1).",
        )?,
    ];
    for (name, spec) in &env.spec().machines {
        for p in spec.ports.iter().filter(|p| p.publish.is_some()) {
            out.push(published(name, &spec.port_label(p), p)?);
        }
    }
    Ok(out)
}

/// Machine `m`'s published port `label`, at the router's address: an HTTP port is an entry.
fn published(m: &str, label: &str, p: &model::Port) -> Result<Output, ManifestError> {
    let snake: String = format!("{m}_{label}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let (role, scheme, end) = match (p.http, p.tls) {
        (true, true) => (OutputRole::Entry, "https://", "/"),
        (true, false) => (OutputRole::Entry, "http://", "/"),
        (false, _) => (OutputRole::Info, "", ""),
    };
    Ok(Output {
        name: SnakeName::parse(&snake)?,
        description: Some(format!("{m}'s {label} (port {}), forwarded by the router.", p.port)),
        role,
        sensitive: false,
        format: Some(Template::from_parts([
            Part::Text(scheme.into()),
            Part::Placeholder(Placeholder::Output(SnakeName::parse(IP)?)),
            Part::Text(":".into()),
            Part::Placeholder(Placeholder::Value),
            Part::Text(end.into()),
        ])),
        from: OutputSource::PublishedPort {
            machine: Name::parse(m)?,
            port: Name::parse(label)?,
        },
    })
}
