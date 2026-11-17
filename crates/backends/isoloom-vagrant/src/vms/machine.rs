//! An environment machine's VM: its box, communicator, networks, published ports, providers, then its
//! planned set-up as provisioners and its checks.

use std::net::Ipv4Addr;

use crate::boxes::{self, MainBox, VagrantBox};
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan, PublishedPort, Vm};
use isoloom_render::GeneratedFile;
use isoloom_spec::model::Arch;

use super::hyperv::{HyperV, Nic};
use super::providers::{self, Guest, Qemu, Size};
use super::qemu::Link;
use super::transport::{self, Os};
use super::utm::Utm;
use super::{Ctx, boot, checks, windows};
use crate::host_folders::Share;
use crate::ruby::{Call, Ruby, Start, Value};
use crate::vagrantfile;

/// Whether environment machine `name` is a VM with a Vagrant box: one the Vagrantfile defines.
pub(crate) fn has_vm(env: &Environment, name: &str) -> bool {
    env.vm(name).and_then(boxes::vagrant).is_some()
}

/// Environment machine `name`'s VM, and the check runner it uploads, if it has checks. Nothing when it
/// has no Vagrant box (refused before rendering).
pub(crate) fn vm(r: &mut Ruby, ctx: &Ctx, name: &str) -> Option<GeneratedFile> {
    let env = ctx.env;
    let planned = env.vm(name)?;
    let mp = planned.plan;
    let image = boxes::vagrant(planned)?;
    let os = Os::of(mp);
    let link = ctx.links.get(name).unwrap_or(&Link::None);
    let group = ctx.checks.of_machine(name).filter(|_| os == Os::Linux);
    let runner = group.map(|g| checks::runner(env, ctx.dir, name, &Position::Machine(name.to_owned()), g));
    // The access machine is defined but not started by `vagrant up`: `vagrant up <name>` starts
    // it, its box overridable (`ISOLOOM_ACCESS_BOX`).
    let access = mp.is_access();
    let start = if access { Start::OnDemand } else { Start::WithUp };
    define(r, name, start, |r| {
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
            let forward = vagrantfile::forwarded_port("m", p.port, host.clone(), p.protocol);
            // Not `fixed`: a free host port when this one is taken (another machine's, a clone's).
            r.call(if p.fixed { forward } else { forward.kw("auto_correct", true) });
        }
        let mut guest = guest(env, planned, &image, link);
        guest.hyperv.published = relays(&published);
        providers::blocks(r, &guest);
        if os == Os::Linux {
            let addresses: Vec<(std::net::Ipv4Addr, u8)> = mp.addresses.iter().map(|(net, ip)| (*ip, env.block(net).prefix())).collect();
            super::addresses::provisioner(r, &addresses);
            super::resolver::provisioner(r);
        }
        let shares: Vec<Share> = mp.host_folders.iter().map(Share::of).collect();
        crate::host_folders::render(r, "m", &shares);
        if os == Os::Linux {
            crate::host_folders::group_writes(r, &shares);
        }
        // Its checks installed (and their on-demand provisioner) before the readiness check,
        // the last provisioner.
        transport::provisioners(r, env, name, os, |r| {
            if let (Some(g), Some(runner)) = (group, &runner) {
                checks::installer(r, name, &runner.contents);
                checks::provisioner(r, name, g);
            }
        });
    });
    runner
}

/// Machine `name`'s published ports, each with its host number, written once here: its forward
/// and Hyper-V's relay of it both take that one. A port the runner may move keeps the one it had
/// when the runner says so ([`vagrantfile::kept_or_chosen_host`]); a fixed one is its own number
/// or nothing ([`vagrantfile::chosen_host`]).
fn published<'a>(name: &str, mp: &'a MachinePlan) -> Vec<(&'a PublishedPort, Value)> {
    mp.ports
        .iter()
        .map(|p| {
            let key = format!("{name}/{}", p.name);
            let host = if p.fixed {
                vagrantfile::chosen_host(&key, p.publish)
            } else {
                vagrantfile::kept_or_chosen_host(name, p.port, p.protocol, &key, p.publish)
            };
            (p, host)
        })
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

/// `config.vm.define "<name>" do |m| ... end`, after a blank line. A machine started
/// [`Start::OnDemand`] (the access box) is defined `autostart: false`, so `vagrant up` skips it
/// and `vagrant up <name>` starts it.
pub(crate) fn define(r: &mut Ruby, name: &str, start: Start, body: impl FnOnce(&mut Ruby)) {
    r.blank();
    r.define(name, start, body);
}

/// The main box of a machine booting `image` on CPU `arch`: its version, and its build pinned
/// only for a non-default (arm64) machine. amd64 is Vagrant's default; setting it makes Vagrant
/// strict-match the box's metadata, which older boxes (`mayfly/windows_server2019`) don't
/// advertise, so they'd be rejected.
pub(crate) fn main_box(image: &VagrantBox, arch: Arch) -> MainBox<'_> {
    MainBox {
        version: image.version.as_deref(),
        arch: (arch != Arch::Amd64).then_some(arch),
        ..MainBox::plain(&image.name)
    }
}

/// The box and its version and architecture. The access box reads its box from the runner's
/// `ISOLOOM_ACCESS_BOX` (its resolved box the fallback), so the runner can give another.
fn boxed(r: &mut Ruby, image: &VagrantBox, arch: Arch, access: bool) {
    let fetched = access.then(|| {
        Value::raw(format!(
            "ENV.fetch({}, {})",
            Value::str(isoloom_spec::contract::ACCESS_BOX),
            Value::str(&image.name)
        ))
    });
    main_box(image, arch).render(r, "m", fetched);
}

/// The guest's hostname, set from `name` on Linux. Windows: no hostname here — Vagrant renames
/// the guest from it over WinRM, which fails on some boxes with a misleading "not a valid name";
/// the environment's own provisioning names it.
fn hostname(r: &mut Ruby, name: &str, os: Os) {
    if os == Os::Linux {
        r.assign("m.vm.hostname", Value::str(name));
    }
}

/// The VM on network `net` at `ip`: a VirtualBox internal network (isolated from the host's
/// LAN) or a libvirt network without DHCP, named after the environment. On an IPv6 network,
/// the libvirt network gets the block too (the host at its `::1`, guests talking IPv6); every
/// provider's VM gets its IPv6 address from the set-up steps.
///
/// The libvirt network is `veryisolated` (between the VMs only, no IPv4 address of the host on
/// it, no way out), unless it serves a network boot, whose DHCP and TFTP the host answers on it.
/// vagrant-libvirt looks a network with an `ip:` up by its address block, and only a
/// `veryisolated` one by its name: on a computer where another network already holds the block
/// (another copy of the environment, another environment on the same block, one left behind) it
/// would take that one and make none, while the VM's interface still names this one, so the VM
/// wouldn't start ("Network not found").
pub(crate) fn private_network(r: &mut Ruby, env: &Environment, net: &str, ip: Ipv4Addr) {
    let netname = crate::vagrantfile::host_name(&env.names().network(net));
    let mut call = Call::new("m.vm.network")
        .arg(Value::str("private_network"))
        .kw("ip", Value::str(&ip.to_string()))
        .kw("netmask", Value::str(&env.block(net).netmask().to_string()))
        .kw("virtualbox__intnet", netname.clone())
        .kw("libvirt__network_name", netname);
    // Every VM on the network carries its network boot: libvirt creates the network with the
    // options of the first VM that comes up on it.
    let boots = isoloom_plan::boot::network_boots(&env.machines);
    let served = boots.get(net);
    if served.is_none() {
        call = call.kw("libvirt__forward_mode", Value::str("veryisolated"));
    }
    call = boot::network(call, served);
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
fn guest<'a>(env: &'a Environment, Vm { name, plan: mp, os: vm_os }: Vm<'a>, image: &'a VagrantBox, link: &'a Link) -> Guest<'a> {
    let os = Os::of(mp);
    // No cloud-init on Windows: its address is set from PowerShell.
    let windows_address = mp
        .addresses
        .first()
        .filter(|_| os == Os::Windows)
        .map(|(net, ip)| (*ip, env.block(net).netmask()));
    let utm = Utm {
        boxed: image.utm.as_deref(),
        nics: mp.addresses.iter().map(|(net, ip)| Nic::on(env, net, *ip)).collect(),
        windows: os == Os::Windows,
    };
    let hyperv = HyperV {
        names: env.names(),
        boxed: image.hyperv.clone(),
        nics: mp.addresses.iter().map(|(net, ip)| Nic::on(env, net, *ip)).collect(),
        windows: os == Os::Windows,
        nested: mp.hardware.nested_virtualization,
        published: Vec::new(),
        tpm: mp.tpm,
        keeps_hostname: false,
    };
    Guest {
        label: env.names().vm_label(name),
        vm_name: env.names().vm_name(name),
        size: Size::of(mp),
        networks: mp.addresses.len(),
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
        utm,
        tpm: mp.tpm,
        esxi_guestos: isoloom_render::esxi::guest_os(vm_os),
        esxi_box: None,
        nested: mp.hardware.nested_virtualization,
        disks: mp.disks.iter().map(|d| (d.name.as_str(), d.size_gb)).collect(),
        console: Some(name),
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
            utm: None,
        };
        let mut r = Ruby::new();
        boxed(&mut r, &image, Arch::Amd64, true);
        assert_eq!(r.finish(), "m.vm.box = ENV.fetch(\"ISOLOOM_ACCESS_BOX\", \"corp/deb\\#{`id`}\\\"\")\n");
    }
}
