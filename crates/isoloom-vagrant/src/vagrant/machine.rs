//! A lab machine's VM: its box, communicator, networks, published ports, providers, then its
//! planned set-up as provisioners and its checks.

use std::net::Ipv4Addr;

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::{GeneratedFile, netmask};
use isoloom_spec::images::{self, VagrantBox};
use isoloom_spec::model::{Arch, Winrm};
use isoloom_spec::validate::Cidr;

use super::hyperv::{HyperV, Nic};
use super::providers::{self, Guest, Qemu, Size};
use super::qemu::Link;
use super::transport::{self, Os};
use super::{Ctx, checks};
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// Lab machine `name`'s VM, and the check runner it uploads, if it has checks. Nothing when it
/// has no Vagrant box (refused before rendering).
pub fn vm(r: &mut Ruby, ctx: &Ctx, name: &str) -> Option<GeneratedFile> {
    let env = ctx.env;
    let mp = env.machines.get(name)?;
    let vm = env.spec().machines.get(name)?.vm.as_ref()?;
    let image = images::vagrant(vm)?;
    let os = if images::is_windows(&vm.os) { Os::Windows } else { Os::Linux };
    let link = ctx.links.get(name).unwrap_or(&Link::None);
    let group = ctx.checks.of_machine(name).filter(|_| os == Os::Linux);
    define(r, name, |r| {
        boxed(r, &image, mp.arch, name, os);
        if os == Os::Windows {
            winrm(r, images::winrm(vm));
        }
        for (net, ip) in &mp.addresses {
            private_network(r, env, net, *ip);
        }
        for p in &mp.ports {
            r.call(vagrantfile::forwarded_port("m", p.port, p.publish, p.protocol));
        }
        providers::blocks(r, &guest(env, name, &image, link, os));
        transport::provisioners(r, env, name, os);
        if let Some(g) = group {
            checks::provisioner(r, name, g);
        }
    });
    group.map(|g| checks::runner(env, ctx.dir, name, &Position::Machine(name.to_string()), g))
}

/// `config.vm.define "<name>" do |m| ... end`, after a blank line.
pub fn define(r: &mut Ruby, name: &str, body: impl FnOnce(&mut Ruby)) {
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

/// The box's own account over WinRM (Windows has no SSH by default). The box forwards RDP to
/// every interface of the host: off (publish a service to reach one, on the loopback).
fn winrm(r: &mut Ruby, transport: Winrm) {
    r.call(
        Call::new("m.vm.network")
            .arg(Value::str("forwarded_port"))
            .kw("guest", 3389u16)
            .kw("host", 3389u16)
            .kw("id", Value::str("rdp"))
            .kw("disabled", true),
    );
    r.assign("m.vm.guest", Value::sym("windows"));
    r.assign("m.vm.communicator", Value::str("winrm"));
    r.assign("m.winrm.username", Value::str("vagrant"));
    r.assign("m.winrm.password", Value::str("vagrant"));
    match transport {
        Winrm::Ssl => {
            r.assign("m.winrm.transport", Value::sym("ssl"));
            r.assign("m.winrm.ssl_peer_verification", false);
        }
        Winrm::Plaintext => {
            r.assign("m.winrm.transport", Value::sym("plaintext"));
            r.assign("m.winrm.basic_auth_only", true);
        }
    }
    r.assign("m.winrm.retry_limit", 30u32);
    r.assign("m.winrm.retry_delay", 10u32);
}

/// The VM on network `net` at `ip`: a VirtualBox internal network (isolated from the host's
/// LAN) or a libvirt network without DHCP, named after the environment.
pub fn private_network(r: &mut Ruby, env: &Environment, net: &str, ip: Ipv4Addr) {
    let netname = format!("isoloom-{}-{net}", env.name);
    r.call(
        Call::new("m.vm.network")
            .arg(Value::str("private_network"))
            .kw("ip", Value::str(&ip.to_string()))
            .kw("netmask", Value::str(&netmask(env.spec(), net).to_string()))
            .kw("virtualbox__intnet", Value::str(&netname))
            .kw("libvirt__network_name", Value::str(&netname))
            .kw("libvirt__dhcp_enabled", false),
    );
}

/// The machine as its providers see it.
fn guest<'a>(env: &'a Environment, name: &str, image: &'a VagrantBox, link: &'a Link, os: Os) -> Guest<'a> {
    let mp: &MachinePlan = &env.machines[name];
    // No cloud-init on Windows: its lab address is set from PowerShell.
    let windows_address = mp
        .addresses
        .first()
        .filter(|_| os == Os::Windows)
        .map(|(net, ip)| (*ip, Cidr::parse(&env.networks[net].cidr).map_or(24, |c| c.len)));
    let hyperv = HyperV {
        env: &env.name,
        boxed: image.hyperv.clone(),
        nics: mp.addresses.iter().map(|(net, ip)| Nic::on(env, net, *ip)).collect(),
        windows: os == Os::Windows,
        nested: env.spec().machines.get(name).is_some_and(|m| m.privileged),
        published: mp.ports.iter().map(|p| (p.port, p.publish)).collect(),
    };
    Guest {
        label: format!("{} · {name}", env.name),
        vm_name: format!("{}-{name}", env.name),
        size: Size::of(mp.size, isoloom_spec::DEFAULT_CPUS, isoloom_spec::DEFAULT_MEMORY_MB),
        nets: mp.addresses.keys().map(String::as_str).collect(),
        libvirt_box: image.libvirt.as_deref(),
        qemu: Qemu {
            boxed: image.qemu.as_deref(),
            arch: Some(mp.arch),
            link,
            windows_address,
        },
        hyperv,
    }
}
