//! Every VM of the Vagrantfile as a manifest machine: the lab's own with their OS, size,
//! addresses and forwarded ports; Isoloom's (the router, the tool shell, the controller) as
//! `infra` or `tool`; each with its define name and its VM's name under every provider that
//! names one (what status and orphan clean-up match, exactly).

use std::collections::BTreeMap;
use std::net::IpAddr;

use isoloom_manifest::{Machine, Name, Native, OsName, Provider, PublishMode, Resources, Role, Template};
use isoloom_plan::{Controller, Environment};
use isoloom_render::run_manifest::{addresses, arch, port};
use isoloom_spec::validate::Cidr;

use super::keeps_controller;
use crate::error::Result;
use crate::vms::{Define, Kind, Shape, label, vm_name};

/// Where a forwarded port listens on the host.
const LOOPBACK: &str = "127.0.0.1";

/// The Vagrantfile's VMs, in its order.
pub(super) fn machines(env: &Environment, shape: &Shape) -> Result<Vec<Machine>> {
    shape.defines.iter().map(|d| machine(env, d)).collect()
}

fn machine(env: &Environment, d: &Define) -> Result<Machine> {
    let mut out = Machine {
        name: Name::parse(&d.name)?,
        role: Role::Infra,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses: BTreeMap::new(),
        keep_running: false,
        native: Some(native(env, d)?),
        volumes: Vec::new(),
        ports: Vec::new(),
    };
    match d.kind {
        Kind::Lab => lab_machine(env, &d.name, &mut out)?,
        Kind::Router => {
            let routed = env.router.iter().flatten().map(|(n, a)| Ok((Name::parse(n)?, IpAddr::V4(*a))));
            out.addresses = routed.collect::<Result<_>>()?;
            out.resources = Some(helper_size());
        }
        Kind::ToolShell => {
            let index = env.spec().tools.keys().position(|t| t == "shell").unwrap_or_default();
            out.role = Role::Tool;
            out.addresses = on_every_network(env, |c| c.tool(index))?;
            out.resources = Some(helper_size());
        }
        Kind::Controller => {
            let size = Controller::of(env.spec());
            out.addresses = on_every_network(env, Cidr::controller)?;
            out.resources = Some(Resources {
                cpus: Some(size.cpus),
                memory_mb: Some(size.memory_mb),
            });
            out.keep_running = keeps_controller(env);
        }
    }
    Ok(out)
}

/// Isoloom's own small VMs: 1 CPU, 512 MB.
fn helper_size() -> Resources {
    Resources {
        cpus: Some(1),
        memory_mb: Some(512),
    }
}

/// A lab machine: its OS, size and addresses, and its ports, published ones forwarded from the
/// host's loopback at their `publish` number (always: Vagrant has no ephemeral forward).
fn lab_machine(env: &Environment, name: &str, out: &mut Machine) -> Result<()> {
    let (Some(mp), Some(m)) = (env.machines.get(name), env.spec().machines.get(name)) else {
        return Ok(());
    };
    out.role = if m.access { Role::Access } else { Role::Target };
    out.os = mp.os.as_deref().map(OsName::parse).transpose()?;
    out.windows = mp.is_windows();
    out.arch = Some(arch(mp.arch));
    let size = crate::vms::providers::Size::of(mp.size, isoloom_spec::DEFAULT_CPUS, isoloom_spec::DEFAULT_MEMORY_MB);
    out.resources = Some(Resources {
        cpus: Some(size.cpus),
        memory_mb: Some(size.mem),
    });
    out.addresses = addresses(mp)?;
    let loopback = Template::text(LOOPBACK);
    for p in &m.ports {
        let mut port = port(&m.port_label(p), p, &mp.ports, &loopback)?;
        if let Some(publish) = &mut port.publish {
            publish.mode = PublishMode::Fixed;
        }
        out.ports.push(port);
    }
    Ok(())
}

/// The VM's address on every network, at `at` of each network's block (Isoloom's own VMs).
fn on_every_network(env: &Environment, at: impl Fn(Cidr) -> std::net::Ipv4Addr) -> Result<BTreeMap<Name, IpAddr>> {
    env.networks
        .iter()
        .filter_map(|(net, n)| Cidr::parse(&n.cidr).map(|c| (net, at(c))))
        .map(|(net, ip)| Ok((Name::parse(net)?, IpAddr::V4(ip))))
        .collect()
}

/// The define name, and the VM's name under each provider that names it: the display name on
/// the desktop hypervisors, `<env>-<what>` on Hyper-V and ESXi, the domain on libvirt (its
/// prefix is the environment's). QEMU names none.
fn native(env: &Environment, d: &Define) -> Result<Native> {
    let (shown, plain) = (label(env, &d.what), vm_name(env, &d.what));
    let vm_names = BTreeMap::from([
        (Provider::Virtualbox, shown.clone()),
        (Provider::VmwareDesktop, shown.clone()),
        (Provider::Parallels, shown.clone()),
        (Provider::Utm, shown),
        (Provider::Hyperv, plain.clone()),
        (Provider::VmwareEsxi, plain),
        (Provider::Libvirt, format!("{}_{}", env.name, d.name)),
    ]);
    Ok(Native::Vagrant {
        vagrant: Name::parse(&d.name)?,
        vm_names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn the_vms_are_named_as_each_provider_names_them_and_the_controller_is_infra() {
        let env = environment(
            "version: 1
name: shop
networks: { lab: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lab: 10 }, os: debian@12, ports: { site: { port: 80, protocol: http, publish: 8086 } }, app: { steps: [{ run: a.sh }] } }
checks: [{ http: 'http://web/', wait: 30 }]
",
        );
        let ms = machines(&env, &Shape::of(&env)).unwrap();
        let names: Vec<&str> = ms.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["web", "isoloom-controller"]);
        let Some(Native::Vagrant { vm_names, .. }) = &ms[0].native else { panic!() };
        assert_eq!(vm_names[&Provider::Virtualbox], "shop · web");
        assert_eq!(vm_names[&Provider::Hyperv], "shop-web");
        assert_eq!(vm_names[&Provider::Libvirt], "shop_web");
        assert!(!vm_names.contains_key(&Provider::Qemu));
        assert_eq!(ms[0].role, Role::Target);
        let site = &ms[0].ports[0];
        assert_eq!(
            site.publish.as_ref().map(|p| (p.mode, p.preferred.map(u16::from))),
            Some((PublishMode::Fixed, Some(8086)))
        );
        assert_eq!((ms[1].role, ms[1].keep_running), (Role::Infra, false));
        assert_eq!(ms[1].addresses[&Name::parse("lab").unwrap()].to_string(), "10.9.0.253");
        let Some(Native::Vagrant { vm_names: controller, .. }) = &ms[1].native else {
            panic!()
        };
        assert_eq!(controller[&Provider::VmwareEsxi], "shop-controller");
    }
}
