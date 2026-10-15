//! A lab machine's VM: its box, communicator, networks, published ports, providers, then its
//! planned set-up as provisioners and its checks.

use std::net::Ipv4Addr;

use isoloom_plan::addressing::netmask;
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::GeneratedFile;
use isoloom_spec::images::{self, VagrantBox};
use isoloom_spec::model::Arch;
use isoloom_spec::validate::Cidr;

use super::hyperv::{HyperV, Nic};
use super::providers::{self, Guest, Qemu, Size};
use super::qemu::Link;
use super::transport::{self, Os};
use super::{Ctx, boot, checks, windows};
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// Whether lab machine `name` is a VM with a Vagrant box: one the Vagrantfile defines.
pub(crate) fn has_vm(env: &Environment, name: &str) -> bool {
    let vm = env.machines.get(name).filter(|mp| mp.shape == isoloom_plan::MachineShape::Vm);
    let spec = env.spec().machines.get(name).and_then(|m| m.vm.as_ref());
    vm.is_some() && spec.and_then(images::vagrant).is_some()
}

/// Lab machine `name`'s VM, and the check runner it uploads, if it has checks. Nothing when it
/// has no Vagrant box (refused before rendering).
pub(crate) fn vm(r: &mut Ruby, ctx: &Ctx, name: &str) -> Option<GeneratedFile> {
    let env = ctx.env;
    let mp = env.machines.get(name)?;
    let vm = env.spec().machines.get(name)?.vm.as_ref()?;
    let image = images::vagrant(vm)?;
    let os = Os::of(mp);
    let link = ctx.links.get(name).unwrap_or(&Link::None);
    let group = ctx.checks.of_machine(name).filter(|_| os == Os::Linux);
    define(r, name, |r| {
        boxed(r, &image, mp.arch, name, os);
        boot::source(r, mp.vm_image.as_ref());
        if os == Os::Windows {
            windows::communicator(r, mp.winrm);
        }
        for (net, ip) in &mp.addresses {
            private_network(r, env, net, *ip);
        }
        super::bridged::networks(r, env, mp);
        for p in &mp.ports {
            r.call(vagrantfile::forwarded_port("m", p.port, p.publish, p.protocol));
        }
        providers::blocks(r, &guest(env, name, mp, &image, link));
        transport::provisioners(r, env, name, os);
        if let Some(g) = group {
            checks::provisioner(r, name, g);
        }
    });
    group.map(|g| checks::runner(env, ctx.dir, name, &Position::Machine(name.to_owned()), g))
}

/// `config.vm.define "<name>" do |m| ... end`, after a blank line.
pub(crate) fn define(r: &mut Ruby, name: &str, body: impl FnOnce(&mut Ruby)) {
    r.blank();
    r.block(Call::new("config.vm.define").arg(Value::str(name)), "m", body);
}

/// The box, its version and architecture, and the hostname.
fn boxed(r: &mut Ruby, image: &VagrantBox, arch: Arch, name: &str, os: Os) {
    r.assign("m.vm.box", Value::str(&image.name));
    if let Some(v) = &image.version {
        r.assign("m.vm.box_version", Value::str(v));
    }
    // Only pinned for non-default (arm64) boxes. amd64 is Vagrant's default; setting it makes
    // Vagrant strict-match the box's metadata, which older boxes (mayfly/windows_server2019)
    // don't advertise, so they'd be rejected.
    if arch != Arch::Amd64 {
        r.assign("m.vm.box_architecture", Value::str(arch.id()));
    }
    // Windows: no hostname here. Vagrant renames the guest from it over WinRM, which fails on
    // some boxes with a misleading "not a valid name"; the lab's own provisioning names it.
    if os == Os::Linux {
        r.assign("m.vm.hostname", Value::str(name));
    }
}

/// The VM on network `net` at `ip`: a VirtualBox internal network (isolated from the host's
/// LAN) or a libvirt network without DHCP, named after the environment. On an IPv6 network,
/// the libvirt network gets the block too (the host at its `::1`, guests talking IPv6); every
/// provider's VM gets its IPv6 address from the set-up steps.
pub(crate) fn private_network(r: &mut Ruby, env: &Environment, net: &str, ip: Ipv4Addr) {
    let netname = super::network_name(&env.name, net);
    let mut call = Call::new("m.vm.network")
        .arg(Value::str("private_network"))
        .kw("ip", Value::str(&ip.to_string()))
        .kw("netmask", Value::str(&netmask(env.spec(), net).to_string()))
        .kw("virtualbox__intnet", Value::str(&netname))
        .kw("libvirt__network_name", Value::str(&netname));
    // Every VM on the network carries its network boot: libvirt creates the network with the
    // options of the first VM that comes up on it.
    call = boot::network(call, isoloom_plan::boot::network_boots(&env.machines).get(net));
    if let Some(v6) = env.networks.get(net).and_then(|n| n.ipv6.as_ref()) {
        call = call
            .kw("libvirt__guest_ipv6", Value::str("yes"))
            .kw("libvirt__ipv6_address", Value::str(&v6.gateway.to_string()))
            .kw("libvirt__ipv6_prefix", Value::str(&isoloom_spec::validate::IPV6_PREFIX.to_string()));
    }
    r.call(call);
}

/// The machine as its providers see it.
fn guest<'a>(env: &'a Environment, name: &str, mp: &'a MachinePlan, image: &'a VagrantBox, link: &'a Link) -> Guest<'a> {
    let os = Os::of(mp);
    // No cloud-init on Windows: its lab address is set from PowerShell.
    let prefix = |net: &str| env.networks.get(net).and_then(|n| Cidr::parse(&n.cidr)).map_or(24, |c| c.len);
    let windows_address = mp.addresses.first().filter(|_| os == Os::Windows).map(|(net, ip)| (*ip, prefix(net)));
    let hyperv = HyperV {
        env: &env.name,
        boxed: image.hyperv.clone(),
        nics: mp.addresses.iter().map(|(net, ip)| Nic::on(env, net, *ip)).collect(),
        windows: os == Os::Windows,
        nested: mp.hardware.nested_virtualization,
        // Windows' port proxy relays TCP only.
        published: mp
            .ports
            .iter()
            .filter(|p| p.protocol != isoloom_spec::model::Protocol::Udp)
            .map(|p| (p.port, p.publish))
            .collect(),
        tpm: mp.tpm,
        keeps_hostname: false,
    };
    Guest {
        label: super::label(env, name),
        vm_name: super::vm_name(env, name),
        size: Size::of(mp.size, isoloom_spec::DEFAULT_CPUS, isoloom_spec::DEFAULT_MEMORY_MB),
        nets: mp.addresses.keys().map(String::as_str).collect(),
        libvirt_box: image.libvirt.as_deref(),
        boot: mp.boot.as_ref(),
        qemu: Qemu {
            boxed: image.qemu.as_deref(),
            main: &image.name,
            arch: Some(mp.arch),
            link,
            windows_address,
        },
        hyperv,
        tpm: mp.tpm,
        esxi_guestos: mp.os.as_deref().and_then(windows::esxi_guestos),
        esxi_box: None,
        nested: mp.hardware.nested_virtualization,
        disks: mp.disks.iter().map(|d| (d.name.as_str(), d.size_gb)).collect(),
    }
}
