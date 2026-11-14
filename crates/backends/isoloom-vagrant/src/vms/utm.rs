//! Vagrant's UTM provider (`vagrant up --provider utm`, vagrant_utm 0.2.0 or later). The plugin
//! makes no adapter for a private network and sets no address in the guest (it has no network
//! step: `private_network` lines are ignored), and boots only UTM-format boxes, so each VM gets,
//! besides its provider block (its UTM box, name, size), triggers that hang on the plugin's own
//! actions (other providers run and print none of them):
//!
//! - before `Boot` (the VM stopped, its configuration editable): an adapter per network, a virtio
//!   card with a fixed MAC ([`hyperv::mac`]) on the network's isolated vmnet network (QEMU's
//!   `vmnet-host` with a `net-uuid` made from the copy's folder and the network's name): the VMs on
//!   one network see each other and nothing else, not the host nor another network, with no DHCP;
//! - after `CheckGuestAdditions` (the guest answers, before its provisioning): the machine's
//!   addresses set in the guest, found by MAC, as on Hyper-V (a boot-time script on Linux, a
//!   static address through WMI on Windows).
//!
//! Published ports are Vagrant's own forwards, which the plugin sets on the box's emulated adapter;
//! the box's shared adapter (UTM's NAT) is the VM's way out. The Ruby doing the work is
//! `IsoloomUtm` ([`UTM_RB`]), in every Vagrantfile's preamble.

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, HostName, MachineShape, Vm};

use super::hyperv::{self, Nic};
use super::providers::Size;
use crate::boxes;
use crate::ruby::{Call, Ruby, Value};

/// The Ruby that gives VMs their networks on UTM (`vagrant_utm.rb`): the `IsoloomUtm` module the
/// triggers call.
pub(crate) const UTM_RB: &str = include_str!("../vagrant_utm.rb");

/// How a VM runs under Vagrant's UTM provider.
#[derive(Debug, Clone)]
pub(crate) struct Utm<'a> {
    /// The UTM box it boots; `None` when its OS has none known (the provider's refusal says so).
    pub(crate) boxed: Option<&'a str>,
    /// Its adapters on the networks, in order.
    pub(crate) nics: Vec<Nic<'a>>,
    /// Whether it runs Windows (its addresses set through WMI, not `ip`).
    pub(crate) windows: bool,
}

impl<'a> Utm<'a> {
    /// Isoloom's own Debian VMs (router, controller, tool shell) at these addresses: Debian 12's
    /// UTM box.
    pub(crate) fn helper(env: &'a Environment, at: impl IntoIterator<Item = (&'a str, Ipv4Addr)>) -> Self {
        Utm {
            boxed: Some(boxes::UTM_DEBIAN_12),
            nics: at.into_iter().map(|(net, ip)| Nic::on(env, net, ip)).collect(),
            windows: false,
        }
    }
}

/// The VM's UTM provider block, then the triggers that give it its networks there. `label` is
/// the name UTM shows; `vm`, whose name without the copy's key seeds its adapters' MAC addresses
/// (the same in every copy, each copy on its own networks).
pub(crate) fn block(r: &mut Ruby, label: &HostName, vm: &HostName, size: Size, utm: &Utm) {
    let seed = vm.unkeyed();
    r.block(Call::new("m.vm.provider").arg(Value::str("utm")), "v, o", |r| {
        match utm.boxed {
            Some(b) => {
                // A UTM box: the main box's version pin and build aren't its own.
                r.assign("o.vm.box", Value::str(b));
                r.assign("o.vm.box_version", Value::str(">= 0"));
                r.assign("o.vm.box_architecture", Value::sym("auto"));
            }
            None => {
                r.comment("No UTM box is known for this OS: give one with `overrides.vagrant.machines.<machine>.boxes.utm`.");
            }
        }
        r.assign("v.name", crate::vagrantfile::host_name(label));
        r.assign("v.cpus", size.cpus);
        r.assign("v.memory", size.mem);
    });
    triggers(r, seed.as_str(), utm);
}

/// `m.trigger.<when> :"VagrantPlugins::Utm::Action::<action>", type: :action do |t|`: only UTM
/// runs that action, so no other provider runs (or prints) the trigger.
fn trigger(r: &mut Ruby, when: &str, action: &str, body: impl FnOnce(&mut Ruby)) {
    let head = Call::new(format!("m.trigger.{when}"))
        .arg(Value::raw(format!(":\"VagrantPlugins::Utm::Action::{action}\"")))
        .kw("type", Value::sym("action"));
    r.block(head, "t", body);
}

/// The adapters before every boot, the addresses once the guest answers.
fn triggers(r: &mut Ruby, vm_name: &str, utm: &Utm) {
    if utm.nics.is_empty() {
        return;
    }
    let nics = Value::array(utm.nics.iter().map(|n| {
        Value::array([
            Value::str(n.net),
            Value::str(&hyperv::linux_mac_text(hyperv::mac(vm_name, n.net))),
        ])
    }));
    trigger(r, "before", "Boot", |r| {
        r.line(format_args!("t.ruby {{ |_, machine| IsoloomUtm.attach(machine, {nics}) }}"));
    });
    trigger(r, "after", "CheckGuestAdditions", |r| {
        r.block("t.ruby", "_, machine", |r| {
            let (script, tag) = if utm.windows {
                (hyperv::windows_addresses(vm_name, &utm.nics), "PS1")
            } else {
                (hyperv::linux_addresses(vm_name, &utm.nics), "SH")
            };
            r.heredoc_argument("IsoloomUtm.address(machine, ", ")", tag, &script);
        });
    });
}

/// Why this environment's VMs can't all boot on UTM, if they can't: the first machine whose OS
/// has no known UTM box and whose spec gives none.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .filter_map(|(name, mp)| Vm::of(name, mp))
        .find_map(|vm| {
            let b = boxes::vagrant(vm)?;
            b.utm.is_none().then(|| {
                format!(
                    "machine `{}`: `{}` has no known UTM box (vagrant_utm boots UTM-format boxes only, `utm/*` for Debian 12 and Ubuntu 24.04); give one with `overrides.vagrant.machines.{}.boxes.utm`",
                    vm.name, vm.os, vm.name
                )
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(spec: &str) -> String {
        let env = crate::testing::environment(spec);
        let mp = env.vm("web").unwrap();
        let utm = Utm {
            boxed: Some(boxes::UTM_DEBIAN_12),
            nics: mp.plan.addresses.iter().map(|(net, ip)| Nic::on(&env, net, *ip)).collect(),
            windows: false,
        };
        let name = env.names().vm_name("web");
        let mut r = Ruby::new();
        block(&mut r, &env.names().vm_label("web"), &name, Size::HELPER, &utm);
        r.finish()
    }

    #[test]
    fn a_vm_boots_its_utm_box_with_an_adapter_per_network_and_its_addresses() {
        let out = rendered(
            "version: 1\nname: t\nnetworks: { front: { cidr: 10.9.0.0/24 }, back: { cidr: 10.9.1.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { front: 10, back: 10 } }\n",
        );
        assert!(out.contains("m.vm.provider \"utm\" do |v, o|"), "{out}");
        assert!(out.contains("o.vm.box = \"utm/bookworm\""), "{out}");
        assert!(out.contains("o.vm.box_architecture = :auto"), "{out}");
        assert!(out.contains(":\"VagrantPlugins::Utm::Action::Boot\", type: :action"), "{out}");
        assert!(out.contains("IsoloomUtm.attach(machine, [[\"front\", \"02:15:"), "{out}");
        assert!(out.contains("[\"back\", \"02:15:"), "{out}");
        assert!(out.contains(":\"VagrantPlugins::Utm::Action::CheckGuestAdditions\", type: :action"), "{out}");
        assert!(out.contains("IsoloomUtm.address(machine, "), "{out}");
        assert!(out.contains("set_address ") && out.contains(" 10.9.0.10/24 "), "{out}");
    }

    #[test]
    fn a_machine_without_a_utm_box_is_refused_with_the_way_out() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@13, networks: { lan: 10 } }\n",
        );
        let why = refusal(&env).unwrap();
        assert!(why.contains("machine `web`") && why.contains("boxes.utm"), "{why}");
        let given = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@13, networks: { lan: 10 } }\noverrides: { vagrant: { machines: { web: { boxes: { utm: my/debian13-utm } } } } }\n",
        );
        assert_eq!(refusal(&given), None);
    }

    #[test]
    fn the_ruby_marks_isoloom_arguments_and_keeps_the_plugins() {
        assert!(UTM_RB.contains("MARK = \"isoloomnet\""));
        assert!(UTM_RB.contains("does not contain mark"));
        assert!(UTM_RB.contains("vmnet-host,id=#{MARK}#{i},net-uuid="));
    }
}
