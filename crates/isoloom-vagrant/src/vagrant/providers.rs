//! Every provider's block for one VM: the same set for the lab's machines and Isoloom's own
//! (router, controller, tool shell), so a provider added here reaches all of them.

use std::net::Ipv4Addr;

use isoloom_spec::images;
use isoloom_spec::model::{Arch, Boot};

use isoloom_spec::windows::Tpm;

use super::boot;
use super::hyperv::{self, HyperV};
use super::qemu::{HubPort, Link};
use super::windows;
use crate::ruby::{Call, Ruby, Value};

/// A VM's size: CPUs, memory (MB), and the disk (GB) when set. The disk is a minimum: a box
/// whose disk is already bigger keeps it (no provider shrinks a disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cpus: u32,
    pub mem: u32,
    pub disk: Option<u32>,
}

impl Size {
    /// Isoloom's own small VMs (router, tool shell): the box's disk.
    pub const HELPER: Size = Size { cpus: 1, mem: 512, disk: None };

    /// A planned size, the spec's defaults filling what it leaves out.
    pub fn of(s: isoloom_plan::Size, cpus: u32, mem: u32) -> Size {
        Size {
            cpus: s.cpus.unwrap_or(cpus),
            mem: s.memory_mb.unwrap_or(mem),
            disk: s.disk_gb,
        }
    }
}

/// How a VM runs under Vagrant's QEMU provider.
#[derive(Debug, Clone)]
pub struct Qemu<'a> {
    /// The libvirt-format box QEMU boots instead of the main one.
    pub boxed: Option<&'a str>,
    /// The main box, which QEMU boots when `boxed` is `None`.
    pub main: &'a str,
    /// The machine's CPU; `None` for Isoloom's own VMs, which run on the host's.
    pub arch: Option<Arch>,
    /// Its link on its private network.
    pub link: &'a Link,
    /// A Windows machine's lab address and prefix length (no cloud-init to set it).
    pub windows_address: Option<(Ipv4Addr, u8)>,
}

impl<'a> Qemu<'a> {
    /// Isoloom's own Debian VMs (router, controller, tool shell): the host's CPU.
    pub fn helper(link: &'a Link) -> Self {
        Qemu {
            boxed: Some(images::QEMU_DEBIAN_12),
            main: images::QEMU_DEBIAN_12,
            arch: None,
            link,
            windows_address: None,
        }
    }
}

/// One VM, as its providers see it.
#[derive(Debug, Clone)]
pub struct Guest<'a> {
    /// Its display name.
    pub label: String,
    /// Its name on ESXi and Hyper-V (`<env>-<machine>`).
    pub vm_name: String,
    /// Its size.
    pub size: Size,
    /// The networks it is on, in order.
    pub nets: Vec<&'a str>,
    /// The box libvirt uses instead of the main one, if any.
    pub libvirt_box: Option<&'a str>,
    /// How it boots, when not the way its box does (libvirt and QEMU).
    pub boot: Option<&'a Boot>,
    /// How QEMU runs it.
    pub qemu: Qemu<'a>,
    /// How Hyper-V runs it.
    pub hyperv: HyperV<'a>,
    /// Its virtual TPM, where the provider gives one (VirtualBox, libvirt; Hyper-V's is in
    /// `hyperv`).
    pub tpm: Option<Tpm>,
    /// The guest OS ESXi registers it as, when its box's own isn't enough (Windows).
    pub esxi_guestos: Option<&'static str>,
}

/// The provider blocks of `g`, after Vagrant's own disk setting (VirtualBox, VMware and Hyper-V
/// resize the box's disk).
pub fn blocks(r: &mut Ruby, g: &Guest) {
    if let Some(d) = g.size.disk {
        r.call(
            Call::new("m.vm.disk")
                .arg(Value::sym("disk"))
                .kw("size", Value::str(&format!("{d}GB")))
                .kw("primary", true),
        );
    }
    desktop(r, "virtualbox", g, true, |r| {
        if g.tpm.is_some() {
            windows::virtualbox_tpm(r);
        }
    });
    vmware(r, g);
    desktop(r, "parallels", g, true, |_| {});
    desktop(r, "utm", g, false, |_| {});
    r.block(provider("qemu"), "v, o", |r| {
        qemu(r, g.size, &g.qemu);
        g.boot.into_iter().for_each(|b| boot::qemu(r, b));
    });
    if let Link::Hub(h) = g.qemu.link {
        hub_triggers(r, h);
    }
    r.block(provider("vmware_esxi"), "v", |r| esxi(r, g));
    libvirt(r, g);
    hyperv::block(r, &g.vm_name, g.size, &g.hyperv);
}

fn provider(name: &str) -> Call {
    Call::new("m.vm.provider").arg(Value::str(name))
}

/// VirtualBox, Parallels and UTM: a name, CPUs and memory (and linked clones where offered),
/// then what only that provider sets (`more`).
fn desktop(r: &mut Ruby, name: &str, g: &Guest, linked_clone: bool, more: impl FnOnce(&mut Ruby)) {
    r.block(provider(name), "v", |r| {
        r.assign("v.name", Value::str(&g.label));
        if linked_clone {
            r.assign("v.linked_clone", true);
        }
        r.assign("v.cpus", g.size.cpus);
        r.assign("v.memory", g.size.mem);
        more(r);
    });
}

fn vmware(r: &mut Ruby, g: &Guest) {
    r.block(provider("vmware_desktop"), "v", |r| {
        r.assign("v.vmx[\"displayName\"]", Value::str(&g.label));
        r.assign("v.vmx[\"numvcpus\"]", Value::str(&g.size.cpus.to_string()));
        r.assign("v.vmx[\"memsize\"]", Value::str(&g.size.mem.to_string()));
        if g.hyperv.nested {
            // Nested virtualization (`privileged`), as Hyper-V's `enable_virtualization_extensions`.
            r.assign("v.vmx[\"vhv.enable\"]", Value::str("TRUE"));
        }
    });
}

/// The CPU architecture as QEMU and libvirt name it.
pub fn qemu_arch(a: Arch) -> &'static str {
    match a {
        Arch::Amd64 => "x86_64",
        Arch::Arm64 => "aarch64",
    }
}

/// libvirt runs a lab machine as its own CPU (emulated when the host's is another), as QEMU
/// does; Isoloom's own VMs take the host's.
fn libvirt(r: &mut Ruby, g: &Guest) {
    let params = if g.libvirt_box.is_some() { "v, o" } else { "v" };
    r.block(provider("libvirt"), params, |r| {
        if let Some(b) = g.libvirt_box {
            r.assign("o.vm.box", Value::str(b));
        }
        r.assign("v.cpus", g.size.cpus);
        r.assign("v.memory", g.size.mem);
        if let Some(a) = g.qemu.arch {
            r.assign("v.machine_arch", Value::str(qemu_arch(a)));
        }
        if let Some(d) = g.size.disk {
            r.assign("v.machine_virtual_size", d);
        }
        if g.tpm.is_some() {
            windows::libvirt_tpm(r);
        }
        g.boot.into_iter().for_each(|b| boot::libvirt(r, b));
    });
}

/// The QEMU (vagrant-qemu) provider. `disk_resize` comes from `IsoloomQemu.disk`: the plugin
/// makes the VM's disk an overlay of that size on the box's, and a size below the box's would
/// truncate it (its file system lost), so it is set only above the box's own size.
fn qemu(r: &mut Ruby, size: Size, q: &Qemu) {
    if let Some(b) = q.boxed {
        // A box of its own: the main box's version pin isn't one of its versions.
        r.assign("o.vm.box", Value::str(b));
        r.assign("o.vm.box_version", Value::str(">= 0"));
    }
    if let Some(a) = q.arch {
        // Its own CPU, whatever the host's (Vagrant otherwise picks the box for the host's).
        r.assign("o.vm.box_architecture", Value::str(a.id()));
        r.assign("v.arch", Value::str(qemu_arch(a)));
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
    if let Some((ip, len)) = q.windows_address {
        let call = Call::new("o.vm.provision").arg(Value::str("shell")).kw("name", Value::str("lab network"));
        r.script(call, "inline", &windows_lab_address(ip, len));
    }
}

fn link(r: &mut Ruby, l: &Link) {
    if let Link::Refused(why) = l {
        r.comment(&format!("No private network on QEMU: {why}."));
    }
    if let Some(opts) = l.socket_opts() {
        if let Link::Hub(h) = l {
            r.comment(&format!("Port {} of network `{}`'s hub (see IsoloomQemu).", h.port, h.net));
        }
        r.assign("v.advanced_network", true);
        r.assign("v.net_mode", Value::sym("socket"));
        r.assign("v.socket_opts", Value::str(&opts));
    }
}

/// The hub of the VM's network, started before QEMU starts the VM and, after the network's
/// first VM is destroyed (the last, as `vagrant destroy` goes), stopped. Action triggers: only
/// QEMU runs these actions, so no other provider runs (or prints) them.
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
    if h.first {
        r.block(trigger("after", "Destroy"), "t", |r| {
            r.line(format_args!("t.ruby {{ |_, _| IsoloomQemu.stop({net}) }}"));
        });
    }
}

/// Emulated (another CPU than the host's) runs many times slower: booting and WinRM get far
/// longer to answer. And on one virtual CPU: QEMU emulates another architecture on a single
/// host thread, so a second vCPU only takes turns with the first (no speed), and code one vCPU
/// rewrites can be seen half-done by the other. .NET rewrites its call stubs at run time; under
/// emulation with 2 vCPUs, Windows' PowerShell died now and then jumping into a stub's data
/// bytes (illegal instruction), which cut WinRM mid-play.
fn emulated(r: &mut Ruby, a: Arch, windows: bool) {
    r.if_block(format_args!("{} != HOST_ARCH", Value::str(qemu_arch(a))), |r| {
        r.assign("v.smp", Value::str("cpus=1"));
        r.assign("o.vm.boot_timeout", 3600u32);
        if windows {
            r.assign("o.winrm.max_tries", 180u32);
            r.assign("o.winrm.timeout", 1800u32);
        }
    });
}

/// PowerShell setting a Windows VM's lab address on QEMU's second adapter: the one without a
/// default gateway (the first is QEMU's user-mode NAT, through which Vagrant reaches it).
fn windows_lab_address(ip: Ipv4Addr, len: u8) -> String {
    format!(
        "$a = Get-NetAdapter | Where-Object {{ -not (Get-NetIPConfiguration -InterfaceIndex $_.ifIndex).IPv4DefaultGateway }} | Select-Object -First 1; \
         if (-not $a) {{ throw 'no second network adapter for the lab network' }}; \
         if (-not (Get-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress {ip} -ErrorAction SilentlyContinue)) {{ \
         Remove-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -Confirm:$false -ErrorAction SilentlyContinue; \
         New-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress {ip} -PrefixLength {len} | Out-Null }}; \
         Set-NetConnectionProfile -InterfaceIndex $a.ifIndex -NetworkCategory Private -ErrorAction SilentlyContinue; \
         \"lab network: {ip}/{len} on $($a.Name)\""
    )
}

/// The vagrant-vmware-esxi provider: the host from the ESXI_* variables, a port group per NIC.
fn esxi(r: &mut Ruby, g: &Guest) {
    let nets = Value::array(g.nets.iter().map(|n| Value::str(n)));
    r.assign("v.esxi_hostname", Value::raw("ENV.fetch(\"ESXI_HOSTNAME\", \"\")"));
    r.assign("v.esxi_hostport", Value::raw("ENV.fetch(\"ESXI_HOSTPORT\", \"22\").to_i"));
    r.assign("v.esxi_username", Value::raw("ENV.fetch(\"ESXI_USERNAME\", \"root\")"));
    r.assign("v.esxi_password", Value::str("env:ESXI_PASSWORD"));
    r.assign_if("v.esxi_disk_store", Value::raw("ENV[\"ESXI_DATASTORE\"]"), "ENV[\"ESXI_DATASTORE\"]");
    r.assign("v.esxi_virtual_network", Value::raw(format!("esxi_networks({nets})")));
    r.assign("v.guest_name", Value::str(&g.vm_name));
    if let Some(os) = g.esxi_guestos {
        r.assign("v.guest_guestos", Value::str(os));
    }
    r.assign("v.guest_numvcpus", g.size.cpus);
    r.assign("v.guest_memsize", g.size.mem);
    if let Some(d) = g.size.disk {
        r.assign("v.guest_boot_disk_size", d);
    }
}
