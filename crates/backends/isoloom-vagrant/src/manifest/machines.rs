//! Every VM of the Vagrantfile as a manifest machine: the lab's own with their OS, size,
//! addresses and forwarded ports; Isoloom's (the router, the tool shell, the controller) as
//! `infra` or `tool`; each with its define name and its VM's name under every provider that
//! names one (what status and orphan clean-up match, exactly), and the digest of what Vagrant
//! applies to it only at boot (`boot_digest`).

use std::collections::BTreeMap;
use std::net::IpAddr;

use isoloom_manifest::{Machine, Name, Native, OsName, Provider, Role, Template};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{addresses, arch, port};
use isoloom_spec::model::Target;
use isoloom_spec::net::Cidr;

use super::keeps_controller;
use crate::error::{Error, Result};
use crate::vms::providers::Size;
use crate::vms::{Define, Kind, Shape, label, vm_name};

/// Where a forwarded port listens on the host.
const LOOPBACK: &str = "127.0.0.1";

/// The Vagrantfile's VMs, in its order, each with its boot digest among `boot_digests` (by define
/// name, [`crate::boot_digest::of`] what its Vagrantfile applies at boot).
pub(super) fn machines(env: &Environment, shape: &Shape, boot_digests: &BTreeMap<String, String>) -> Result<Vec<Machine>> {
    shape
        .defines
        .iter()
        .map(|d| {
            let digest = boot_digests.get(&d.name).ok_or_else(|| Error::NoBootDigest {
                target: Target::Vagrant,
                machine: d.name.clone(),
            })?;
            machine(env, d, digest)
        })
        .collect()
}

fn machine(env: &Environment, d: &Define, boot_digest: &str) -> Result<Machine> {
    let mut out = isoloom_render::run_manifest::machine(&d.name, Role::Infra)?;
    out.native = Some(native(env, d, boot_digest)?);
    match d.kind {
        Kind::Lab => lab_machine(env, &d.name, &mut out)?,
        Kind::Router => {
            let routed = env.router.iter().flatten().map(|(n, a)| Ok((Name::parse(n)?, IpAddr::V4(*a))));
            out.addresses = routed.collect::<Result<_>>()?;
            out.resources = Some(Size::HELPER.resources());
        }
        Kind::ToolShell => {
            let index = env.spec().tools.keys().position(|t| t == "shell").unwrap_or_default();
            out.role = Role::Tool;
            out.addresses = on_every_network(env, |c| c.tool(index))?;
            out.resources = Some(Size::HELPER.resources());
        }
        Kind::Controller => {
            out.addresses = on_every_network(env, Cidr::controller)?;
            out.resources = Some(Size::controller(env.spec()).resources());
            out.keep_running = keeps_controller(env);
        }
    }
    Ok(out)
}

/// A lab machine: its OS, size and addresses, and its ports. A published port is forwarded from
/// the host's loopback to a port the runner picks free at the first start (reflected in the lab's
/// URL), unless the spec fixed it (`publish: { fixed: true }`), when it forwards that number.
fn lab_machine(env: &Environment, name: &str, out: &mut Machine) -> Result<()> {
    let (Some(mp), Some(m)) = (env.machines.get(name), env.spec().machines.get(name)) else {
        return Ok(());
    };
    out.role = if m.access { Role::Access } else { Role::Member };
    out.on_demand = m.access;
    out.os = mp.os.as_deref().map(OsName::parse).transpose()?;
    out.windows = mp.is_windows();
    out.arch = Some(arch(mp.arch));
    out.resources = Some(isoloom_render::run_manifest::resources(mp));
    out.addresses = addresses(mp)?;
    let loopback = Template::text(LOOPBACK);
    for p in &m.ports {
        // The host port is the runner's to choose (ephemeral), unless the spec fixed it: the
        // shared `port` builder reads that from the plan's `fixed` flag.
        out.ports.push(port(&m.port_label(p), p, &mp.ports, &loopback)?);
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

/// A VM's name under each provider that names one: the display name (`shown`) on the desktop
/// hypervisors, `plain` (`<env>-<what>`) on Hyper-V and ESXi, `libvirt` (the domain, its prefix
/// the environment's) on libvirt. QEMU names none.
pub(super) fn vm_names(shown: String, plain: String, libvirt: String) -> BTreeMap<Provider, String> {
    BTreeMap::from([
        (Provider::Virtualbox, shown.clone()),
        (Provider::VmwareDesktop, shown.clone()),
        (Provider::Parallels, shown.clone()),
        (Provider::Utm, shown),
        (Provider::Hyperv, plain.clone()),
        (Provider::VmwareEsxi, plain),
        (Provider::Libvirt, libvirt),
    ])
}

fn native(env: &Environment, d: &Define, boot_digest: &str) -> Result<Native> {
    let (shown, plain) = (label(env, &d.name), vm_name(env, &d.name));
    let vm_names = vm_names(shown, plain, format!("{}_{}", env.name, d.name));
    Ok(Native::Vagrant {
        vagrant: Name::parse(&d.name)?,
        vm_names,
        boot_digest: Some(boot_digest.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use isoloom_manifest::PublishMode;

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
        let digests = BTreeMap::from([
            ("web".to_owned(), "sha256:0".to_owned()),
            ("isoloom-controller".to_owned(), "sha256:1".to_owned()),
        ]);
        let ms = machines(&env, &Shape::of(&env), &digests).unwrap();
        let names: Vec<&str> = ms.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["web", "isoloom-controller"]);
        let Some(Native::Vagrant { vm_names, boot_digest, .. }) = &ms[0].native else {
            panic!()
        };
        assert_eq!(boot_digest.as_deref(), Some("sha256:0"));
        assert_eq!(vm_names[&Provider::Virtualbox], "shop - web");
        assert_eq!(vm_names[&Provider::Hyperv], "shop-web");
        assert_eq!(vm_names[&Provider::Libvirt], "shop_web");
        assert!(!vm_names.contains_key(&Provider::Qemu));
        assert_eq!(ms[0].role, Role::Member);
        let site = &ms[0].ports[0];
        // Not fixed in the spec, so the host port is the runner's to pick (ephemeral); 8086 is the
        // fallback it defaults to when the runner picked none.
        assert_eq!(
            site.publish.as_ref().map(|p| (p.mode, p.preferred.map(u16::from))),
            Some((PublishMode::Ephemeral, Some(8086)))
        );
        assert_eq!((ms[1].role, ms[1].keep_running), (Role::Infra, false));
        assert_eq!(ms[1].addresses[&Name::parse("lab").unwrap()].to_string(), "10.9.0.253");
        let Some(Native::Vagrant { vm_names: controller, .. }) = &ms[1].native else {
            panic!()
        };
        assert_eq!(controller[&Provider::VmwareEsxi], "shop-isoloom-controller");
    }

    #[test]
    fn a_vm_without_a_boot_digest_is_a_bug_not_a_manifest_without_one() {
        let env = environment("version: 1\nname: shop\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lab: 10 }, os: debian@12 }\n");
        let err = machines(&env, &Shape::of(&env), &BTreeMap::new()).unwrap_err();
        assert!(matches!(&err, Error::NoBootDigest { machine, .. } if machine == "web"), "{err}");
        assert_eq!(err.to_string(), "vagrant: the Vagrantfile records nothing it applies at boot to `web`");
    }
}
