//! A Linux VM's addresses on the environment's networks under VMware Workstation and Fusion, set
//! by Isoloom at every start and never written into the guest.
//!
//! Left to Vagrant (`auto_config`), each address is written into the guest's network
//! configuration (`auto eth1` in Debian's `/etc/network/interfaces`). At the next boot that card
//! is up before the NAT card's DHCP answers, VMware Tools reports its address as the VM's,
//! vagrant-vmware-desktop forwards SSH to it, and Vagrant's network step then takes the card down
//! (`ifdown eth1`) to configure it again, closing its own session: the VM never came back from a
//! pause or a forced stop (seen on Fusion, arm64 and `x86_64` Macs, with vagrant-vmware-desktop
//! 3.0.5).
//!
//! So on VMware a Linux VM's private networks are `auto_config: false`
//! ([`machine::private_network`](super::machine::private_network)), each adapter gets a MAC of its
//! own ([`vmx`]), and a trigger once the VM answers, at every start (a resume too: `vagrant up
//! --no-provision` runs no provisioner), finds each adapter by its MAC and gives it its address
//! ([`trigger`]). Nothing is left for the next boot to bring up early.

use isoloom_render::stable_hash;

use super::hyperv::{self, Nic};
use super::nested;
use super::provider::Provider;
use super::providers::{self, Guest};
use crate::console;
use crate::ruby::{Call, Ruby, Value};

/// The MAC of VM `vm`'s adapter on `net`: VMware's range for addresses set by hand
/// (`00:50:56:00:00:00` to `00:50:56:3f:ff:ff`), the rest seeded by the VM's name (without the
/// copy's key) and the network's.
pub(crate) fn mac(vm: &str, net: &str) -> [u8; 6] {
    let [a, b, c, _] = stable_hash::of(&format!("{vm}/{net}")).to_be_bytes();
    [0x00, 0x50, 0x56, a & 0x3f, b, c]
}

/// In the VMware block: adapter `ethernet<n>`'s MAC ([`mac`]), `n` from 1 in the order of `nics`.
/// vagrant-vmware-desktop clears every `ethernet<n>` key at each start, then applies these.
pub(crate) fn vmx(r: &mut Ruby, vm: &str, nics: &[Nic]) {
    for (i, n) in nics.iter().enumerate() {
        let slot = i + 1;
        r.assign(&format!("v.vmx[\"ethernet{slot}.addressType\"]"), Value::str("static"));
        r.assign(
            &format!("v.vmx[\"ethernet{slot}.address\"]"),
            Value::str(&hyperv::linux_mac_text(mac(vm, n.net))),
        );
    }
}

/// The script that gives each adapter (found by its MAC) its address now, NetworkManager told to
/// leave the adapters alone (it would ask DHCP there, which these networks don't have).
pub(crate) fn script(vm: &str, nics: &[Nic]) -> String {
    let hyperv::Adapters { lines, unmanaged, .. } = hyperv::adapters(nics, |net| hyperv::linux_mac_text(mac(vm, net)));
    format!(
        "{}if [ -d /etc/NetworkManager/conf.d ]; then\n  printf '[keyfile]\\nunmanaged-devices={unmanaged}\\n' > /etc/NetworkManager/conf.d/90-isoloom-env-net.conf\nfi\n{lines}exit $rc\n",
        hyperv::ENV_NET
    )
}

/// `m.trigger.after` vagrant-vmware-desktop's port forwarding, its last step of each start before
/// Vagrant waits for the guest (only VMware runs it, so no other provider runs or prints the
/// trigger): once the guest answers, the addresses ([`script`]), before any provisioner. An
/// action's `after` trigger runs before the steps after it, so not after the network step, which
/// comes before the VM boots. Nothing for a VM on none of the environment's networks.
pub(crate) fn trigger(r: &mut Ruby, vm: &str, nics: &[Nic]) {
    if nics.is_empty() {
        return;
    }
    let head = Call::new("m.trigger.after")
        .arg(Value::raw(":\"HashiCorp::VagrantVMwareDesktop::Action::ForwardPorts\""))
        .kw("type", Value::sym("action"));
    let script = script(vm, nics);
    r.block(head, "t", |r| {
        r.block("t.ruby", "_, machine", |r| {
            r.line(format_args!(
                "raise Vagrant::Errors::VMBootTimeout unless machine.communicate.wait_for_ready(machine.config.vm.boot_timeout)"
            ));
            r.heredoc_argument("machine.communicate.sudo(", ")", "SH", &script);
        });
    });
}

/// VMware Workstation and Fusion: a display name, CPUs and memory, in the VM's `.vmx`, and each
/// adapter on the environment's networks at a PCI slot of its own ([`pci_slot`]). A Linux VM's
/// adapters also get MACs of their own and their addresses from a trigger ([`trigger`]).
pub(super) fn block(r: &mut Ruby, g: &Guest) {
    let seed = g.vm_name.unkeyed();
    let linux = (!g.hyperv.windows).then_some(g.hyperv.nics.as_slice());
    r.block(providers::provider(Provider::VmwareDesktop), "v, o", |r| {
        providers::primary_disk(r, g.size);
        workstation_pro(r);
        r.assign("v.vmx[\"displayName\"]", crate::vagrantfile::host_name(&g.label));
        r.assign("v.vmx[\"numvcpus\"]", Value::str(&g.size.cpus.to_string()));
        r.assign("v.vmx[\"memsize\"]", Value::str(&g.size.mem.to_string()));
        for n in 1..=g.networks {
            r.assign(&format!("v.vmx[\"ethernet{n}.pcislotnumber\"]"), Value::str(&pci_slot(n).to_string()));
        }
        if let Some(nics) = linux {
            vmx(r, seed.as_str(), nics);
        }
        if g.nested {
            nested::switch(r, Provider::VmwareDesktop);
        }
        if let Some(machine) = g.console {
            console::vmware(r, machine);
        }
    });
    if let Some(nics) = linux {
        trigger(r, seed.as_str(), nics);
    }
}

/// Off a Mac, Workstation run as the Pro product it is. Broadcom's free Workstation Pro (since
/// 17.6) reports a `standard` licence, which vagrant-vmware-desktop 3.0.5 still takes for
/// Workstation Player: it then drives VMware as `vmrun -T player`, whose `snapshot` (each linked
/// clone's first step) fails with "The operation is not supported", so no VM ever started (seen on
/// Workstation 26.0.1, Debian 13). Fusion is told from its product name, not its licence, and is
/// left as it is.
fn workstation_pro(r: &mut Ruby) {
    r.if_block("!RUBY_PLATFORM.include?(\"darwin\")", |r| {
        r.assign("v.force_vmware_license", Value::str("workstation"));
    });
}

/// The PCI slot of VMware adapter `ethernet<n>` (`n` from 1: the box's own NAT adapter is
/// `ethernet0`, left as the box has it). Without one, VMware picks a slot as the adapter is
/// added, so attaching the second network's adapter (or a boot after a shutdown) moved the
/// guest's interfaces under other names, and its SSH session closed. Each adapter takes the first
/// function of a PCIe root port of its own (192, 224, 256), then the next functions of those
/// ports: the slots VMware itself gives added adapters.
pub(crate) fn pci_slot(n: usize) -> usize {
    let i = n.saturating_sub(1);
    192 + 32 * (i % 3) + i / 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn nics() -> Vec<Nic<'static>> {
        vec![
            Nic {
                net: "lan",
                ip: Ipv4Addr::new(10, 80, 0, 10),
                len: 24,
                mask: Ipv4Addr::new(255, 255, 255, 0),
            },
            Nic {
                net: "dmz",
                ip: Ipv4Addr::new(10, 81, 0, 9),
                len: 28,
                mask: Ipv4Addr::new(255, 255, 255, 240),
            },
        ]
    }

    #[test]
    fn each_adapter_has_a_mac_in_vmwares_static_range_of_its_own() {
        for vm in ["web", "db", "isoloom-router"] {
            let a = mac(vm, "lan");
            assert_eq!(a[..3], [0x00, 0x50, 0x56]);
            assert!(a[3] <= 0x3f, "{a:?}");
            assert_ne!(a, mac(vm, "dmz"));
        }
        assert_eq!(mac("web", "lan"), mac("web", "lan"));
    }

    #[test]
    fn the_addresses_are_set_once_the_vm_answers_and_written_nowhere_for_boot() {
        let mut r = Ruby::new();
        trigger(&mut r, "web", &nics());
        let out = r.finish();
        assert!(
            out.contains("m.trigger.after :\"HashiCorp::VagrantVMwareDesktop::Action::ForwardPorts\", type: :action do |t|"),
            "{out}"
        );
        assert!(out.contains("machine.communicate.sudo(<<~'SH')"), "{out}");
        assert!(
            out.contains(&format!("set_address {} 10.80.0.10/24", hyperv::linux_mac_text(mac("web", "lan")))),
            "{out}"
        );
        assert!(
            !out.contains("/etc/network/interfaces") && !out.contains("systemctl enable"),
            "nothing for boot: {out}"
        );
        let mut none = Ruby::new();
        trigger(&mut none, "web", &[]);
        assert_eq!(none.finish().trim(), "");
    }

    #[test]
    fn the_vmx_gives_each_adapter_its_mac() {
        let mut r = Ruby::new();
        vmx(&mut r, "web", &nics());
        let out = r.finish();
        assert!(out.contains("v.vmx[\"ethernet1.addressType\"] = \"static\""), "{out}");
        assert!(
            out.contains(&format!("v.vmx[\"ethernet2.address\"] = \"{}\"", hyperv::linux_mac_text(mac("web", "dmz")))),
            "{out}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_script_parses() {
        let ok = std::process::Command::new("sh").args(["-n", "-c", &script("web", &nics())]).status().unwrap();
        assert!(ok.success());
    }
}
