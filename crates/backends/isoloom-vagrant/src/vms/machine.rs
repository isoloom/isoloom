//! A lab machine's VM: its box, communicator, networks, published ports, providers, then its
//! planned set-up as provisioners and its checks.

use std::net::Ipv4Addr;

use crate::boxes::{self, VagrantBox};
use isoloom_manifest as manifest;
use isoloom_plan::addressing::netmask;
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan, PublishedPort};
use isoloom_render::GeneratedFile;
use isoloom_spec::model::Arch;
use isoloom_spec::net::Cidr;

use super::hyperv::{HyperV, Nic};
use super::providers::{self, Guest, Qemu, Size};
use super::qemu::Link;
use super::transport::{self, Os};
use super::{Ctx, boot, checks, windows};
use crate::host_folders::Share;
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// Whether lab machine `name` is a VM with a Vagrant box: one the Vagrantfile defines.
pub(crate) fn has_vm(env: &Environment, name: &str) -> bool {
    let vm = env.machines.get(name).filter(|mp| mp.shape == isoloom_plan::MachineShape::Vm);
    let spec = env.spec().machines.get(name).and_then(|m| m.vm.as_ref());
    vm.is_some() && spec.and_then(boxes::vagrant).is_some()
}

/// Lab machine `name`'s VM, and the check runner it uploads, if it has checks. Nothing when it
/// has no Vagrant box (refused before rendering).
pub(crate) fn vm(r: &mut Ruby, ctx: &Ctx, name: &str) -> Option<GeneratedFile> {
    let env = ctx.env;
    let mp = env.machines.get(name)?;
    let vm = env.spec().machines.get(name)?.vm.as_ref()?;
    let image = boxes::vagrant(vm)?;
    let os = Os::of(mp);
    let link = ctx.links.get(name).unwrap_or(&Link::None);
    let group = ctx.checks.of_machine(name).filter(|_| os == Os::Linux);
    // The access machine is defined but not started by `vagrant up`: the runner hot-adds it,
    // overriding its box with one it is given.
    let access = env.spec().machines.get(name).is_some_and(|m| m.access);
    define(r, name, access, |r| {
        boxed(r, &image, mp.arch, access);
        hostname(r, name, os);
        boot::source(r, mp.vm_image.as_ref());
        if os == Os::Windows {
            windows::communicator(r, mp.winrm);
        }
        for (net, ip) in &mp.addresses {
            private_network(r, env, net, *ip);
        }
        super::bridged::networks(r, env, mp);
        let published = published(name, mp);
        for (p, host) in &published {
            r.call(vagrantfile::forwarded_port("m", p.port, host.clone(), p.protocol));
        }
        let mut guest = guest(env, name, mp, &image, link);
        guest.hyperv.published = relays(&published);
        providers::blocks(r, &guest);
        let shares: Vec<Share> = mp.host_folders.iter().map(Share::of).collect();
        crate::host_folders::render(r, "m", &shares);
        transport::provisioners(r, env, name, os);
        if let Some(g) = group {
            checks::provisioner(r, name, g);
        }
    });
    group.map(|g| checks::runner(env, ctx.dir, name, &Position::Machine(name.to_owned()), g))
}

/// Machine `name`'s published ports, each with its host number ([`vagrantfile::chosen_host`]),
/// written once here: its forward and Hyper-V's relay of it both take that one.
fn published<'a>(name: &str, mp: &'a MachinePlan) -> Vec<(&'a PublishedPort, Value)> {
    mp.ports
        .iter()
        .map(|p| (p, vagrantfile::chosen_host(&format!("{name}/{}", p.name), p.publish)))
        .collect()
}

/// What Hyper-V relays of the `published` ports (it forwards none): each TCP one, from the host
/// number its forward takes. Windows' port proxy relays TCP only.
fn relays(published: &[(&PublishedPort, Value)]) -> Vec<(u16, Value)> {
    published
        .iter()
        .filter(|(p, _)| p.protocol != isoloom_spec::model::Protocol::Udp)
        .map(|(p, host)| (p.port, host.clone()))
        .collect()
}

/// `config.vm.define "<name>" do |m| ... end`, after a blank line. An `on_demand` machine (the
/// access box) is defined `autostart: false`, so `vagrant up` skips it and the runner hot-adds it.
pub(crate) fn define(r: &mut Ruby, name: &str, on_demand: bool, body: impl FnOnce(&mut Ruby)) {
    r.blank();
    r.define(name, !on_demand, body);
}

/// The box and its version and architecture. The access box reads its box from the runner's
/// `ISOLOOM_ACCESS_BOX` (its resolved box the fallback), so the runner can give another.
fn boxed(r: &mut Ruby, image: &VagrantBox, arch: Arch, access: bool) {
    if access {
        let fetch = format!("ENV.fetch({}, {})", Value::str(manifest::access::BOX_ENV), Value::str(&image.name));
        r.assign("m.vm.box", Value::raw(fetch));
    } else {
        r.assign("m.vm.box", Value::str(&image.name));
    }
    if let Some(v) = &image.version {
        r.assign("m.vm.box_version", Value::str(v));
    }
    // Only pinned for non-default (arm64) boxes. amd64 is Vagrant's default; setting it makes
    // Vagrant strict-match the box's metadata, which older boxes (mayfly/windows_server2019)
    // don't advertise, so they'd be rejected.
    if arch != Arch::Amd64 {
        r.assign("m.vm.box_architecture", Value::str(arch.id()));
    }
}

/// The guest's hostname, set from `name` on Linux. Windows: no hostname here — Vagrant renames
/// the guest from it over WinRM, which fails on some boxes with a misleading "not a valid name";
/// the lab's own provisioning names it.
fn hostname(r: &mut Ruby, name: &str, os: Os) {
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
            .kw("libvirt__ipv6_prefix", Value::str(&isoloom_spec::net::IPV6_PREFIX.to_string()));
    }
    r.call(call);
}

/// The machine as its providers see it, but for Hyper-V's relays of its published ports
/// ([`relays`]), which take the host numbers of its forwards.
fn guest<'a>(env: &'a Environment, name: &str, mp: &'a MachinePlan, image: &'a VagrantBox, link: &'a Link) -> Guest<'a> {
    let os = Os::of(mp);
    // No cloud-init on Windows: its lab address is set from PowerShell.
    let prefix = |net: &str| env.networks.get(net).and_then(|n| Cidr::parse(&n.cidr)).map_or(24, Cidr::prefix);
    let windows_address = mp.addresses.first().filter(|_| os == Os::Windows).map(|(net, ip)| (*ip, prefix(net)));
    let hyperv = HyperV {
        env: &env.name,
        boxed: image.hyperv.clone(),
        nics: mp.addresses.iter().map(|(net, ip)| Nic::on(env, net, *ip)).collect(),
        windows: os == Os::Windows,
        nested: mp.hardware.nested_virtualization,
        published: Vec::new(),
        tpm: mp.tpm,
        keeps_hostname: false,
    };
    Guest {
        label: super::label(env, name),
        vm_name: super::vm_name(env, name),
        size: Size::of(mp),
        port_groups: super::port_groups(env, mp.addresses.keys().map(String::as_str)),
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
        esxi_guestos: mp.os.as_deref().and_then(isoloom_render::esxi::guest_os),
        esxi_box: None,
        nested: mp.hardware.nested_virtualization,
        disks: mp.disks.iter().map(|d| (d.name.as_str(), d.size_gb)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::HyperVBox;

    #[test]
    fn the_access_box_is_a_ruby_string_that_interpolates_nothing() {
        let image = VagrantBox {
            name: "corp/deb#{`id`}\"".to_owned(),
            version: None,
            libvirt: None,
            qemu: None,
            hyperv: HyperVBox::Main,
        };
        let mut r = Ruby::new();
        boxed(&mut r, &image, Arch::Amd64, true);
        assert_eq!(r.finish(), "m.vm.box = ENV.fetch(\"ISOLOOM_ACCESS_BOX\", \"corp/deb\\#{`id`}\\\"\")\n");
    }
}
