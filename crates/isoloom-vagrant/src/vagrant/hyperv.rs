//! Vagrant's Hyper-V provider (`vagrant up --provider hyperv`, from an elevated shell). Vagrant
//! puts every adapter of a VM on one switch and sets no address in the guest, so each VM gets,
//! besides its provider block (name, size, a linked clone, nested virtualization when
//! `nested_virtualization`), triggers that hang on Hyper-V's own actions (other providers run and print
//! none of them):
//!
//! - before `Configure`: the first adapter's switch (`ISOLOOM_HYPERV_SWITCH`, else the Default
//!   Switch) must exist, and each network is a private switch `isoloom-<env>-<network>`;
//! - before `StartInstance`: an adapter per network with a fixed MAC ([`mac`]), connected there,
//!   and the virtual TPM (`vm.tpm`, or Windows 11's own) enabled with a key protector of the host;
//! - after `WaitForIPAddress`: the machine's addresses set in the guest, found by MAC (a
//!   boot-time script on Linux, a static address through WMI on Windows), and a warning saying
//!   where the published ports are (Hyper-V forwards none);
//! - after `DeleteVM`: the networks' switches no VM uses any more, removed.
//!
//! The Ruby doing the work is `IsoloomHyperV` ([`HYPERV_RB`]), in every Vagrantfile's preamble.

use std::fmt::Write as _;

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, MachineShape};
use isoloom_render::netmask;
use isoloom_spec::images::{self, HyperVBox};
use isoloom_spec::validate::Cidr;
use isoloom_spec::windows::Tpm;

use super::providers::Size;
use crate::ruby::{Call, Ruby, Value};

/// The Ruby that gives VMs their networks on Hyper-V (`vagrant_hyperv.rb`): `HYPERV_SWITCH`
/// and the `IsoloomHyperV` module the triggers call.
pub const HYPERV_RB: &str = include_str!("../vagrant_hyperv.rb");

/// How a VM runs under Vagrant's Hyper-V provider.
#[derive(Debug, Clone)]
pub struct HyperV<'a> {
    /// The environment's name (its networks' switches are named after it).
    pub env: &'a str,
    /// The box Hyper-V boots.
    pub boxed: HyperVBox,
    /// Its adapters on the networks, in order.
    pub nics: Vec<Nic<'a>>,
    /// Whether it runs Windows (its addresses set through WMI, not `ip`).
    pub windows: bool,
    /// Nested virtualization (`nested_virtualization`): the VM runs VMs of its own.
    pub nested: bool,
    /// Its published ports (guest, host): not forwarded on Hyper-V, so a warning says where.
    pub published: Vec<(u16, u16)>,
    /// Its virtual TPM, enabled before it starts.
    pub tpm: Option<Tpm>,
}

/// A VM's adapter on one network, and its address there.
#[derive(Debug, Clone, Copy)]
pub struct Nic<'a> {
    /// The network.
    pub net: &'a str,
    /// The VM's address on it.
    pub ip: Ipv4Addr,
    /// The network's prefix length.
    pub len: u8,
    /// The network's mask (Windows' WMI takes a mask).
    pub mask: Ipv4Addr,
}

impl<'a> Nic<'a> {
    /// The adapter on `net` at `ip`.
    pub fn on(env: &Environment, net: &'a str, ip: Ipv4Addr) -> Nic<'a> {
        Nic {
            net,
            ip,
            len: env.networks.get(net).and_then(|n| Cidr::parse(&n.cidr)).map_or(24, |c| c.len),
            mask: netmask(env.spec(), net),
        }
    }
}

impl<'a> HyperV<'a> {
    /// Isoloom's own Debian VMs (router, tool shell) at these addresses: Debian 12's Hyper-V box.
    pub fn helper(env: &'a Environment, at: impl IntoIterator<Item = (&'a str, Ipv4Addr)>) -> Self {
        HyperV {
            env: &env.name,
            boxed: HyperVBox::Other {
                name: images::HYPERV_DEBIAN_12.0.into(),
                version: Some(images::HYPERV_DEBIAN_12.1.into()),
            },
            nics: at.into_iter().map(|(net, ip)| Nic::on(env, net, ip)).collect(),
            windows: false,
            nested: false,
            published: Vec::new(),
            tpm: None,
        }
    }

    /// The private switch of a network.
    fn switch(&self, net: &str) -> String {
        format!("isoloom-{}-{net}", self.env)
    }

    /// The networks' switches, as a Ruby array.
    fn switches(&self) -> Value {
        Value::array(self.nics.iter().map(|n| Value::str(&self.switch(n.net))))
    }
}

/// The MAC address of a VM's adapter on a network under Hyper-V: locally administered, stable
/// for the VM and the network, so the guest finds the adapter whatever it names it.
pub fn mac(vm: &str, net: &str) -> [u8; 6] {
    // FNV-1a: stable across builds and platforms (unlike std's hasher).
    let mut h: u32 = 0x811c_9dc5;
    for b in format!("{vm}/{net}").bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    let b = h.to_be_bytes();
    [0x02, 0x15, b[0], b[1], b[2], b[3]]
}

/// A MAC address written with `sep` between its bytes, in capitals when `upper` (Hyper-V's
/// cmdlets and WMI) or not (Linux's `/sys/class/net/*/address`).
fn mac_text(mac: [u8; 6], sep: &str, upper: bool) -> String {
    let s = mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(sep);
    if upper { s.to_uppercase() } else { s }
}

/// The VM `vm_name`'s Hyper-V provider block, then the triggers that give it its networks there.
pub fn block(r: &mut Ruby, vm_name: &str, size: Size, hv: &HyperV) {
    r.block(Call::new("m.vm.provider").arg(Value::str("hyperv")), "v, o", |r| {
        match &hv.boxed {
            HyperVBox::Main => {}
            HyperVBox::Other { name, version } => {
                // Another box (or version) than the main one: the main one's pin isn't its.
                r.assign("o.vm.box", Value::str(name));
                r.assign("o.vm.box_version", Value::str(version.as_deref().unwrap_or(">= 0")));
            }
            HyperVBox::Missing => {
                r.comment("No Hyper-V build of this box is known: give one with `vm.image.hyperv`.");
            }
        }
        r.assign("v.vmname", Value::str(vm_name));
        r.assign("v.cpus", size.cpus);
        r.assign("v.memory", size.mem);
        r.assign("v.linked_clone", true);
        if hv.nested {
            super::nested::switch(r, "hyperv");
        }
        // Hyper-V reads only `bridge` here: the switch of the VM's first adapter.
        let first = Call::new("o.vm.network").arg(Value::str("private_network"));
        r.call(first.kw("bridge", Value::raw("HYPERV_SWITCH")));
    });
    triggers(r, vm_name, hv);
}

/// `m.trigger.<when> :"VagrantPlugins::HyperV::Action::<action>", type: :action do |t|`: only
/// Hyper-V runs that action, so no other provider runs (or prints) the trigger.
fn trigger(r: &mut Ruby, when: &str, action: &str, body: impl FnOnce(&mut Ruby)) {
    let head = Call::new(format!("m.trigger.{when}"))
        .arg(Value::raw(format!(":\"VagrantPlugins::HyperV::Action::{action}\"")))
        .kw("type", Value::sym("action"));
    r.block(head, "t", body);
}

/// The switches before Vagrant picks the first adapter's, the adapters before the VM starts,
/// its addresses once it answers (and where its published ports are), the switches gone with
/// the last VM.
fn triggers(r: &mut Ruby, vm_name: &str, hv: &HyperV) {
    let switches = hv.switches();
    trigger(r, "before", "Configure", |r| {
        r.line(format_args!("t.ruby {{ |_, _| IsoloomHyperV.switches({switches}) }}"));
    });
    let nics = Value::array(
        hv.nics
            .iter()
            .map(|n| Value::array([Value::str(&hv.switch(n.net)), Value::str(&mac_text(mac(vm_name, n.net), "", true))])),
    );
    let mut before_start = Vec::new();
    if !hv.nics.is_empty() {
        before_start.push(format!("IsoloomHyperV.attach(machine, {nics})"));
    }
    if let Some(t) = hv.tpm {
        before_start.push(format!("IsoloomHyperV.tpm(machine, {})", t == Tpm::Required));
    }
    if !before_start.is_empty() {
        trigger(r, "before", "StartInstance", |r| {
            r.line(format_args!("t.ruby {{ |_, machine| {} }}", before_start.join("; ")));
        });
    }
    if hv.nics.is_empty() && hv.published.is_empty() {
        return;
    }
    trigger(r, "after", "WaitForIPAddress", |r| {
        r.block("t.ruby", "_, machine", |r| {
            if !hv.nics.is_empty() {
                let (script, tag) = if hv.windows {
                    (windows_addresses(vm_name, &hv.nics), "PS1")
                } else {
                    (linux_addresses(vm_name, &hv.nics), "SH")
                };
                r.heredoc_line(format_args!("IsoloomHyperV.address(machine, <<~'{tag}')"), tag, &script);
            }
            if !hv.published.is_empty() {
                let ports = Value::array(hv.published.iter().map(|(g, h)| Value::array([Value::from(*g), Value::from(*h)])));
                r.line(format_args!("IsoloomHyperV.ports(machine, {ports})"));
            }
        });
    });
    if !hv.nics.is_empty() {
        trigger(r, "after", "DeleteVM", |r| {
            r.line(format_args!("t.ruby {{ |_, _| IsoloomHyperV.prune({switches}) }}"));
        });
    }
}

/// The script that sets a Linux VM's addresses at boot (`/etc/isoloom/lab-net.sh`): each
/// adapter found by its MAC, its address set with `ip` (NetworkManager told to leave it
/// alone). Every network is tried; a missing adapter fails it.
const LAB_NET: &str = "#!/bin/sh\n# This machine's addresses on the isoloom networks (Hyper-V sets none), by adapter MAC.\nlab() {\n  for d in /sys/class/net/*; do\n    [ \"$(cat \"$d/address\" 2>/dev/null)\" = \"$1\" ] || continue\n    IF=${d##*/}\n    if command -v nmcli >/dev/null; then nmcli device set \"$IF\" managed no 2>/dev/null || true; fi\n    ip link set \"$IF\" up\n    ip -4 addr show dev \"$IF\" | grep -q \" $2 \" || ip addr add \"$2\" dev \"$IF\"\n    return 0\n  done\n  echo \"isoloom: no network adapter with MAC $1\" >&2\n  return 1\n}\nrc=0\n";

/// Runs `lab-net.sh` at every boot: a systemd unit, or OpenRC's `local` on Alpine.
const LAB_NET_AT_BOOT: &str = "if [ -d /run/systemd/system ]; then\n  cat > /etc/systemd/system/isoloom-lab-net.service <<'UNIT'\n[Unit]\nDescription=Addresses on the isoloom networks\nAfter=network-pre.target NetworkManager.service systemd-networkd.service\nBefore=network-online.target isoloom-routes.service\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/etc/isoloom/lab-net.sh\n\n[Install]\nWantedBy=multi-user.target\nUNIT\n  systemctl daemon-reload\n  systemctl enable -q isoloom-lab-net.service\nelif command -v rc-update >/dev/null; then\n  printf '#!/bin/sh\\nexec /etc/isoloom/lab-net.sh\\n' > /etc/local.d/isoloom-lab-net.start\n  chmod +x /etc/local.d/isoloom-lab-net.start\n  rc-update add local default >/dev/null\nfi\n";

/// A Linux VM's addresses on its networks under Hyper-V: `lab-net.sh` written, run at every
/// boot, and now.
fn linux_addresses(vm_name: &str, nics: &[Nic]) -> String {
    let macs: Vec<String> = nics.iter().map(|n| mac_text(mac(vm_name, n.net), ":", false)).collect();
    let lines = nics.iter().zip(&macs).fold(String::new(), |mut s, (n, m)| {
        let _ = writeln!(s, "lab {m} {}/{} || rc=1", n.ip, n.len);
        s
    });
    let unmanaged = macs.iter().map(|m| format!("mac:{m}")).collect::<Vec<_>>().join(";");
    format!(
        "set -e\nmkdir -p /etc/isoloom\ncat > /etc/isoloom/lab-net.sh <<'NET'\n{LAB_NET}{lines}exit $rc\nNET\nchmod +x /etc/isoloom/lab-net.sh\n# NetworkManager would try DHCP there.\nif [ -d /etc/NetworkManager/conf.d ]; then\n  printf '[keyfile]\\nunmanaged-devices={unmanaged}\\n' > /etc/NetworkManager/conf.d/90-isoloom-lab.conf\nfi\n{LAB_NET_AT_BOOT}/etc/isoloom/lab-net.sh\n"
    )
}

/// A Windows VM's addresses on its networks under Hyper-V: each adapter found by its MAC, a
/// static address (kept across restarts) set through WMI, as on every Windows since 2008 R2,
/// its network then Private where the cmdlet exists.
fn windows_addresses(vm_name: &str, nics: &[Nic]) -> String {
    let items = nics
        .iter()
        .map(|n| {
            format!(
                "@{{ mac = '{}'; ip = '{}'; mask = '{}' }}",
                mac_text(mac(vm_name, n.net), ":", true),
                n.ip,
                n.mask
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "$ErrorActionPreference = 'Stop'\nforeach ($n in @({items})) {{\n  $c = Get-WmiObject Win32_NetworkAdapterConfiguration | Where-Object {{ $_.MACAddress -eq $n.mac }} | Select-Object -First 1\n  if (-not $c) {{ throw ('no network adapter with MAC ' + $n.mac) }}\n  if (@($c.IPAddress) -notcontains $n.ip) {{\n    $r = $c.EnableStatic(@($n.ip), @($n.mask))\n    if ($r.ReturnValue -gt 1) {{ throw ('setting ' + $n.ip + ' failed: ' + $r.ReturnValue) }}\n  }}\n  if (Get-Command Set-NetConnectionProfile -ErrorAction SilentlyContinue) {{ Set-NetConnectionProfile -InterfaceIndex $c.InterfaceIndex -NetworkCategory Private -ErrorAction SilentlyContinue }}\n  'lab network: ' + $n.ip + ' on ' + $c.Description\n}}\n"
    )
}

/// Why this environment's VMs can't all boot on Hyper-V, if they can't: the first machine whose
/// OS has no known Hyper-V box and whose spec gives none.
pub fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    env.machines.iter().filter(|(_, mp)| mp.shape == MachineShape::Vm).find_map(|(name, _)| {
        let vm = spec.machines.get(name)?.vm.as_ref()?;
        let b = images::vagrant(vm)?;
        (b.hyperv == HyperVBox::Missing).then(|| {
            format!(
                "machine `{name}`: `{}` has no known Hyper-V build of its box; give one with `vm.image.hyperv`",
                vm.os
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{mac, mac_text};

    #[test]
    fn macs_are_local_stable_and_differ_by_vm_and_network() {
        let a = mac("lab-web", "front");
        assert_eq!(a, mac("lab-web", "front"));
        assert_eq!(&a[..2], &[0x02, 0x15]);
        assert_ne!(a, mac("lab-web", "back"));
        assert_ne!(a, mac("lab-db", "front"));
        assert_eq!(mac_text([0x02, 0x15, 0xab, 0, 1, 0xff], ":", false), "02:15:ab:00:01:ff");
        assert_eq!(mac_text([0x02, 0x15, 0xab, 0, 1, 0xff], "", true), "0215AB0001FF");
    }
}
