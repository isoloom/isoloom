//! Each VM's link under Vagrant's QEMU provider. The plugin gives a VM one private network, a
//! QEMU `socket` netdev. Multicast, the plugin's other no-root mode, doesn't send on a Mac
//! (Darwin refuses to send from a socket bound to the group), so every network gets a hub: one
//! QEMU process with no machine, a `listen` port per VM on one emulated hub, each VM connecting
//! to its own ([`QEMU_RB`]). vmnet, TAP and `socket_vmnet` would need root (or a daemon running
//! as root): Isoloom uses none of them.
//!
//! The hub ports are the host's, so they belong to the copy of the project that runs, not to the
//! spec: two copies of one environment (two folders, one name) run side by side. The Vagrantfile
//! picks them when Vagrant reads it (`IsoloomQemu.ports`), free on the host and kept while a hub
//! runs on them; a hub listens on spare ones too, which a VM joining its network while it runs
//! takes, so it is never restarted under the VMs connected to it. What Isoloom writes is which VM
//! has which network's port.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::router;

use std::net::Ipv4Addr;

use isoloom_spec::model::Arch;

use super::provider::Provider;
use super::providers::{self, Guest, Size};
use super::{CONTROLLER, TOOL_SHELL, boot, windows};
use crate::boxes;
use crate::console;
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// The Ruby the QEMU provider's hubs and disk sizes use (`vagrant_qemu.rb`): the
/// `IsoloomQemu` module, in the preamble of a Vagrantfile that needs it ([`needs_ruby`]).
pub(crate) const QEMU_RB: &str = include_str!("../vagrant_qemu.rb");

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Link {
    /// On no network.
    None,
    /// A port of its network's hub, which the VM connects to.
    Hub(HubPort),
    /// Why QEMU can't give it its network (the VM then has none there).
    Refused(String),
}

/// A VM's port on its network's hub: `QEMU_HUBS[net][vm]`, picked when Vagrant reads the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HubPort {
    /// The network, as `QEMU_HUBS` keys it.
    pub(crate) net: String,
    /// The VM, by its define name, as `QEMU_HUBS[net]` keys it.
    pub(crate) vm: String,
}

impl Link {
    /// The `socket` netdev options the VM runs with, when it has a link: a Ruby string, its port
    /// read from `QEMU_HUBS`.
    pub(crate) fn socket_opts(&self) -> Option<Value> {
        match self {
            Link::Hub(h) => Some(Value::raw(format!(
                "\"connect=127.0.0.1:#{{QEMU_HUBS[{}][{}]}}\"",
                Value::str(&h.net),
                Value::str(&h.vm)
            ))),
            Link::None | Link::Refused(_) => None,
        }
    }
}

/// Every VM of the Vagrantfile in the order it is defined (and started), with its networks:
/// the router, the machines, the tool shell, the controller.
fn vms_and_networks(env: &Environment, with_controller: bool) -> Vec<(String, Vec<&str>)> {
    let all: Vec<&str> = env.networks.keys().map(String::as_str).collect();
    let mut vms = Vec::new();
    if let Some(r) = &env.router {
        vms.push((router::NAME.to_owned(), r.keys().map(String::as_str).collect()));
    }
    let machines = env.start_order.iter().filter_map(|name| Some((name, env.machines.get(name)?)));
    for (name, m) in machines.filter(|(_, m)| m.shape == MachineShape::Vm) {
        vms.push((name.clone(), m.addresses.keys().map(String::as_str).collect()));
    }
    if env.tools.contains_key("shell") {
        vms.push((TOOL_SHELL.to_owned(), all.clone()));
    }
    if with_controller {
        vms.push((CONTROLLER.to_owned(), all));
    }
    vms
}

/// Each VM's link, by its define name: a VM on one network gets a port of its hub, one host port
/// per linked VM, at most [`HUB_PORT_COUNT`] of them.
pub(crate) fn links(env: &Environment, with_controller: bool) -> IndexMap<String, Link> {
    let mut linked = 0;
    vms_and_networks(env, with_controller)
        .into_iter()
        .map(|(name, nets)| {
            let link = match nets.as_slice() {
                [] => Link::None,
                [net] => {
                    linked += 1;
                    if linked <= usize::from(HUB_PORT_COUNT) {
                        Link::Hub(HubPort {
                            net: (*net).to_owned(),
                            vm: name.clone(),
                        })
                    } else {
                        Link::Refused(format!("`{name}`: QEMU links at most {HUB_PORT_COUNT} VMs of an environment, a host port each"))
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name, link)
        })
        .collect()
}

/// Each network with a hub, and its VMs, in the order they are defined: the hub listens on a port
/// for each, and stops once none of them is left.
pub(crate) fn hubs(links: &IndexMap<String, Link>) -> IndexMap<String, Vec<String>> {
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    for link in links.values() {
        if let Link::Hub(h) = link {
            out.entry(h.net.clone()).or_default().push(h.vm.clone());
        }
    }
    out
}

/// Whether the Vagrantfile needs [`QEMU_RB`]: a network has a hub, or a VM a disk size.
pub(crate) fn needs_ruby(links: &IndexMap<String, Link>, sized: bool) -> bool {
    sized || links.values().any(|l| matches!(l, Link::Hub(_)))
}

/// How many host ports the hubs pick from: `IsoloomQemu::PORTS` in [`QEMU_RB`], which a test
/// holds to this value.
const HUB_PORT_COUNT: u16 = 20_000;

/// The first VM that can't have its network, as the reason.
pub(crate) fn refusal(links: IndexMap<String, Link>) -> Option<String> {
    links.into_values().find_map(|l| match l {
        Link::Refused(why) => Some(why),
        _ => None,
    })
}

/// How a VM runs under Vagrant's QEMU provider.
#[derive(Debug, Clone)]
pub(crate) struct Qemu<'a> {
    /// The libvirt-format box QEMU boots instead of the main one.
    pub(crate) boxed: Option<&'a str>,
    /// The main box, which QEMU boots when `boxed` is `None`.
    pub(crate) main: &'a str,
    /// The machine's CPU; `None` for Isoloom's own VMs, which run on the host's.
    pub(crate) arch: Option<Arch>,
    /// Its link on its private network.
    pub(crate) link: &'a Link,
    /// A Windows machine's address on its first network, and the network's mask (no cloud-init
    /// to set it).
    pub(crate) windows_address: Option<(Ipv4Addr, Ipv4Addr)>,
}

impl<'a> Qemu<'a> {
    /// Isoloom's own Debian VMs (router, controller, tool shell): the host's CPU.
    pub(crate) fn helper(link: &'a Link) -> Self {
        Qemu {
            boxed: Some(boxes::QEMU_DEBIAN_12),
            main: boxes::QEMU_DEBIAN_12,
            arch: None,
            link,
            windows_address: None,
        }
    }
}

/// The CPU architecture as QEMU and libvirt name it.
pub(crate) fn arch(a: Arch) -> &'static str {
    match a {
        Arch::Amd64 => "x86_64",
        Arch::Arm64 => "aarch64",
    }
}

/// The QEMU provider's block, then the triggers of the hub of the VM's network, if it is on one.
pub(super) fn block(r: &mut Ruby, g: &Guest) {
    r.block(providers::provider(Provider::Qemu), "v, o", |r| {
        settings(r, g.size, &g.qemu);
        g.boot.into_iter().for_each(|b| boot::qemu(r, b));
        if g.console.is_some() {
            console::qemu(r, g.qemu.arch);
        }
    });
    if let Link::Hub(h) = g.qemu.link {
        hub_triggers(r, h);
    }
}

/// The QEMU (vagrant-qemu) provider. `disk_resize` comes from `IsoloomQemu.disk`: the plugin
/// makes the VM's disk an overlay of that size on the box's, and a size below the box's would
/// truncate it (its file system lost), so it is set only above the box's own size.
fn settings(r: &mut Ruby, size: Size, q: &Qemu) {
    if let Some(b) = q.boxed {
        // A box of its own: the main box's version pin isn't one of its versions.
        r.assign("o.vm.box", Value::str(b));
        r.assign("o.vm.box_version", Value::str(">= 0"));
    }
    if let Some(a) = q.arch {
        // Its own CPU, whatever the host's (Vagrant otherwise picks the box for the host's).
        r.assign("o.vm.box_architecture", Value::str(a.id()));
        r.assign("v.arch", Value::str(arch(a)));
    }
    r.assign("v.smp", Value::str(&format!("cpus={}", size.cpus)));
    r.assign("v.memory", Value::str(&format!("{}M", size.mem)));
    // Each VM forwards SSH from its own host port (the plugin's default is one fixed port).
    r.assign("v.ssh_auto_correct", true);
    if let Some(gb) = size.disk {
        let main = Value::str(q.boxed.unwrap_or(q.main));
        r.assign("v.disk_resize", Value::raw(format!("IsoloomQemu.disk({main}, {gb})")));
    }
    link(r, q.link);
    if let Some(a) = q.arch {
        emulated(r, a, q.windows_address.is_some());
    }
    if let Some((ip, mask)) = q.windows_address {
        // QEMU's first adapter is its user-mode NAT, the way Vagrant comes in: the environment's
        // network is the one without a gateway.
        let nic = [(windows::Adapter::NoGateway, ip, mask)];
        r.provision(vagrantfile::shell("o", "environment network").script("inline", &windows::static_addresses(&nic)));
    }
}

fn link(r: &mut Ruby, l: &Link) {
    if let Link::Refused(why) = l {
        r.comment(&format!("No private network on QEMU: {why}."));
    }
    if let (Some(opts), Link::Hub(h)) = (l.socket_opts(), l) {
        r.comment(&format!("Its port of network `{}`'s hub (see IsoloomQemu).", h.net));
        r.assign("v.advanced_network", true);
        r.assign("v.net_mode", Value::sym("socket"));
        r.assign("v.socket_opts", opts);
    }
}

/// The hub of the VM's network, started before QEMU starts the VM (unless it runs on the ports
/// this read of the Vagrantfile picked, `QEMU_HUBS[net]`) and stopped
/// once the network's last VM is destroyed, whichever VM that is: `vagrant destroy <vm>` alone
/// leaves the hub to the VMs still there. Action triggers: only QEMU runs these actions, so no
/// other provider runs (or prints) them.
fn hub_triggers(r: &mut Ruby, h: &HubPort) {
    let net = Value::str(&h.net);
    let trigger = |when: &str, action: &str| {
        Call::new(format!("m.trigger.{when}"))
            .arg(Value::raw(format!(":\"VagrantPlugins::QEMU::Action::{action}\"")))
            .kw("type", Value::sym("action"))
    };
    r.block(trigger("before", "StartInstance"), "t", |r| {
        r.line(format_args!("t.ruby {{ |_, _| IsoloomQemu.hub({net}, QEMU_HUBS[{net}]) }}"));
    });
    r.block(trigger("after", "Destroy"), "t", |r| {
        r.line(format_args!(
            "t.ruby {{ |env, machine| IsoloomQemu.release({net}, QEMU_HUBS[{net}].keys, env, machine.name) }}"
        ));
    });
}

/// Emulated (another CPU than the host's) runs many times slower: booting and WinRM get far
/// longer to answer. And on one virtual CPU: QEMU emulates another architecture on a single
/// host thread, so a second vCPU only takes turns with the first (no speed), and code one vCPU
/// rewrites can be seen half-done by the other. .NET rewrites its call stubs at run time; under
/// emulation with 2 vCPUs, Windows' PowerShell died now and then jumping into a stub's data
/// bytes (illegal instruction), which cut WinRM mid-play.
fn emulated(r: &mut Ruby, a: Arch, windows: bool) {
    r.if_block(format_args!("{} != HOST_ARCH", Value::str(arch(a))), |r| {
        r.assign("v.smp", Value::str("cpus=1"));
        r.assign("o.vm.boot_timeout", 3600u32);
        if windows {
            r.assign("o.winrm.max_tries", 180u32);
            r.assign("o.winrm.timeout", 1800u32);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{HubPort, Link};

    #[test]
    fn the_hubs_port_count_is_the_rubys() {
        let ruby = super::QEMU_RB
            .lines()
            .find_map(|l| l.trim().strip_prefix("PORTS = "))
            .map(|n| n.replace('_', "").parse::<u16>());
        assert_eq!(ruby, Some(Ok(super::HUB_PORT_COUNT)), "vagrant_qemu.rb's PORTS and qemu.rs's differ");
    }

    fn hub(net: &str, vm: &str) -> Link {
        Link::Hub(HubPort {
            net: net.into(),
            vm: vm.into(),
        })
    }

    #[test]
    fn hubs_name_every_vm_of_the_network() {
        let links: indexmap::IndexMap<String, Link> = [
            ("r".to_owned(), hub("app", "r")),
            ("lone".to_owned(), Link::None),
            ("a".to_owned(), hub("app", "a")),
            ("other".to_owned(), Link::Refused("on two networks".into())),
            ("b".to_owned(), hub("app", "b")),
        ]
        .into_iter()
        .collect();
        let hubs = super::hubs(&links);
        // Every VM is named, not only the first: the hub stops once none of them is left.
        assert_eq!(hubs["app"], ["r", "a", "b"]);
        assert_eq!(hubs.len(), 1);
    }

    #[test]
    fn every_network_is_a_hub_even_with_two_vms() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { os: debian@12, networks: { lab: 10 } }\n  b: { os: debian@12, networks: { lab: 11 } }\n",
        );
        let links = super::links(&env, false);
        assert_eq!(links["a"], hub("lab", "a"));
        assert_eq!(links["b"], hub("lab", "b"));
        assert_eq!(super::hubs(&links)["lab"], ["a", "b"]);
    }

    /// The port is the copy's, read when Vagrant reads the file: nothing of the spec's name.
    #[test]
    fn a_vm_connects_to_its_port_as_the_vagrantfile_picked_it() {
        let opts = hub("app", "web").socket_opts().map(|v| v.to_string());
        assert_eq!(opts.as_deref(), Some(r#""connect=127.0.0.1:#{QEMU_HUBS["app"]["web"]}""#));
        assert_eq!(Link::None.socket_opts(), None);
    }
}
