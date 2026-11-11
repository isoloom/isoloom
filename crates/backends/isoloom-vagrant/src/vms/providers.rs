//! Every provider's block for one VM: the same set for the environment's machines and Isoloom's own
//! (router, controller, tool shell), so a provider added here reaches all of them.

use std::net::Ipv4Addr;

use crate::boxes::{self, Choice, MainBox};
use crate::console;
use isoloom_spec::model::{Arch, Boot};
use isoloom_spec::windows::Tpm;

use super::hyperv::{self, HyperV};
use super::provider::Provider;
use super::qemu::{HubPort, Link};
use super::{boot, nested, windows};
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// A VM's size: CPUs, memory (MB), and the disk (GB) when set. The disk is a minimum: a box
/// whose disk is already bigger keeps it (no provider shrinks a disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) cpus: u32,
    pub(crate) mem: u32,
    pub(crate) disk: Option<u32>,
}

impl Size {
    /// Isoloom's own small VMs (router, tool shell): the box's disk.
    pub(crate) const HELPER: Size = Size { cpus: 1, mem: 512, disk: None };

    /// The controller's: the plan's controller's sizes (the spec's `controller`, else its
    /// defaults); the defaults when the environment has none.
    pub(crate) fn controller(c: Option<&isoloom_plan::Controller>) -> Size {
        Size {
            cpus: c.map_or(isoloom_plan::Controller::CPUS, |c| c.cpus),
            mem: c.map_or(isoloom_plan::Controller::MEMORY_MB, |c| c.memory_mb),
            disk: c.and_then(|c| c.disk_gb),
        }
    }

    /// Its CPUs and memory, as a run manifest gives a machine's.
    pub(crate) fn resources(self) -> isoloom_manifest::Resources {
        isoloom_manifest::Resources {
            cpus: Some(self.cpus),
            memory_mb: Some(self.mem),
        }
    }

    /// An environment machine's size as it runs ([`isoloom_plan::MachinePlan::size`]); its disk only when
    /// the spec sets one (else the box's).
    pub(crate) fn of(mp: &isoloom_plan::MachinePlan) -> Size {
        let size = mp.size();
        Size {
            cpus: size.cpus,
            mem: size.memory_mb,
            disk: mp.resources.disk_gb,
        }
    }
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

/// One VM, as its providers see it.
#[derive(Debug, Clone)]
pub(crate) struct Guest<'a> {
    /// Its display name.
    pub(crate) label: String,
    /// Its name on ESXi and Hyper-V (`<env>-<machine>`).
    pub(crate) vm_name: String,
    /// Its size.
    pub(crate) size: Size,
    /// The ESXi port group of each network it is on, in order, by its index in
    /// `ESXI_VIRTUAL_NETWORK` ([`super::port_groups`]).
    pub(crate) port_groups: Vec<usize>,
    /// The box libvirt uses instead of the main one, if any.
    pub(crate) libvirt_box: Option<&'a str>,
    /// How it boots, when not the way its box does (libvirt and QEMU).
    pub(crate) boot: Option<&'a Boot>,
    /// How QEMU runs it.
    pub(crate) qemu: Qemu<'a>,
    /// How Hyper-V runs it.
    pub(crate) hyperv: HyperV<'a>,
    /// Its virtual TPM, where the provider gives one (VirtualBox, libvirt; Hyper-V's is in
    /// `hyperv`).
    pub(crate) tpm: Option<Tpm>,
    /// The guest OS ESXi registers it as, when its box's own isn't enough (Windows).
    pub(crate) esxi_guestos: Option<&'static str>,
    /// The box ESXi boots instead of the main one, if any: the controller's Alpine box reports no
    /// address through its VMware Tools there, so `vagrant up` would wait for one forever.
    pub(crate) esxi_box: Option<&'a str>,
    /// Whether it runs VMs of its own (`nested_virtualization`).
    pub(crate) nested: bool,
    /// Its data disks, `(name, GB)`, in order.
    pub(crate) disks: Vec<(&'a str, u32)>,
    /// Its machine's name, when it is one of the environment's: it gets a graphical console
    /// ([`crate::console`]) where the provider gives one. Isoloom's own VMs get none.
    pub(crate) console: Option<&'a str>,
}

impl Guest<'_> {
    /// The boxes the VM boots, provider by provider: `main` ([`MainBox`]), then the box each
    /// provider block of this guest boots instead, as [`blocks`] writes them.
    pub(crate) fn choice<'b>(&'b self, main: MainBox<'b>) -> Choice<'b> {
        Choice {
            main,
            libvirt: self.libvirt_box,
            qemu: self.qemu.boxed,
            cpu: self.qemu.arch,
            hyperv: Some(&self.hyperv.boxed),
            esxi: self.esxi_box,
            utm: None,
        }
    }
}

/// The provider blocks of `g`, after Vagrant's own disk setting (VirtualBox, VMware and Hyper-V
/// resize the box's disk).
pub(crate) fn blocks(r: &mut Ruby, g: &Guest) {
    if let Some(d) = g.size.disk {
        r.call(
            Call::new("m.vm.disk")
                .arg(Value::sym("disk"))
                .kw("size", Value::str(&format!("{d}GB")))
                .kw("primary", true),
        );
    }
    // Data disks: Vagrant's own setting for VirtualBox, VMware and Hyper-V; libvirt and ESXi
    // in their blocks. QEMU and Parallels attach none (refused for them).
    for (name, gb) in &g.disks {
        r.call(
            Call::new("m.vm.disk")
                .arg(Value::sym("disk"))
                .kw("name", Value::str(name))
                .kw("size", Value::str(&format!("{gb}GB"))),
        );
    }
    desktop(r, Provider::VirtualBox, g, |r| {
        if g.tpm.is_some() {
            windows::virtualbox_tpm(r);
        }
        if g.console.is_some() {
            console::virtualbox(r);
        }
    });
    vmware(r, g);
    desktop(r, Provider::Parallels, g, |_| {});
    r.block(provider(Provider::Qemu), "v, o", |r| {
        qemu(r, g.size, &g.qemu);
        g.boot.into_iter().for_each(|b| boot::qemu(r, b));
        if g.console.is_some() {
            console::qemu(r, g.qemu.arch);
        }
    });
    if let Link::Hub(h) = g.qemu.link {
        hub_triggers(r, h);
    }
    let params = if g.esxi_box.is_some() { "v, o" } else { "v" };
    r.block(provider(Provider::VmwareEsxi), params, |r| esxi(r, g));
    libvirt(r, g);
    hyperv::block(r, &g.vm_name, g.size, &g.hyperv);
    if g.nested {
        nested::refuse_elsewhere(r);
    }
}

/// `m.vm.provider "<id>"`: the head of a provider's block.
fn provider(p: Provider<'_>) -> Call {
    Call::new("m.vm.provider").arg(Value::str(p.id()))
}

/// VirtualBox and Parallels: a name, CPUs, memory and linked clones, then what only that
/// provider sets (`more`).
fn desktop(r: &mut Ruby, p: Provider<'_>, g: &Guest, more: impl FnOnce(&mut Ruby)) {
    r.block(provider(p), "v", |r| {
        r.assign("v.name", Value::str(&g.label));
        r.assign("v.linked_clone", true);
        r.assign("v.cpus", g.size.cpus);
        r.assign("v.memory", g.size.mem);
        more(r);
        if g.nested {
            nested::switch(r, p);
        }
    });
}

/// VMware Workstation and Fusion: a display name, CPUs and memory, in the VM's `.vmx`.
fn vmware(r: &mut Ruby, g: &Guest) {
    r.block(provider(Provider::VmwareDesktop), "v", |r| {
        r.assign("v.vmx[\"displayName\"]", Value::str(&g.label));
        r.assign("v.vmx[\"numvcpus\"]", Value::str(&g.size.cpus.to_string()));
        r.assign("v.vmx[\"memsize\"]", Value::str(&g.size.mem.to_string()));
        if g.nested {
            nested::switch(r, Provider::VmwareDesktop);
        }
        if let Some(machine) = g.console {
            console::vmware(r, machine);
        }
    });
}

/// The CPU architecture as QEMU and libvirt name it.
pub(crate) fn qemu_arch(a: Arch) -> &'static str {
    match a {
        Arch::Amd64 => "x86_64",
        Arch::Arm64 => "aarch64",
    }
}

/// libvirt runs an environment machine as its own CPU (emulated when the host's is another), as QEMU
/// does; Isoloom's own VMs take the host's.
fn libvirt(r: &mut Ruby, g: &Guest) {
    let params = if g.libvirt_box.is_some() { "v, o" } else { "v" };
    r.block(provider(Provider::Libvirt), params, |r| {
        if let Some(b) = g.libvirt_box {
            r.assign("o.vm.box", Value::str(b));
            // A box of its own: the main box's version pin isn't one of its versions (as for QEMU).
            r.assign("o.vm.box_version", Value::str(">= 0"));
            // A roboxes box, with a build for each architecture and each one's metadata: the
            // machine's own, else (Isoloom's own VMs) the host's.
            let arch = g.qemu.arch.map_or_else(|| Value::sym("auto"), |a| Value::str(a.id()));
            r.assign("o.vm.box_architecture", arch);
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
        if g.nested {
            nested::switch(r, Provider::Libvirt);
        }
        for (_, gb) in &g.disks {
            r.call(Call::new("v.storage").arg(Value::sym("file")).kw("size", Value::str(&format!("{gb}G"))));
        }
        if g.console.is_some() {
            console::libvirt(r);
        }
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
    r.if_block(format_args!("{} != HOST_ARCH", Value::str(qemu_arch(a))), |r| {
        r.assign("v.smp", Value::str("cpus=1"));
        r.assign("o.vm.boot_timeout", 3600u32);
        if windows {
            r.assign("o.winrm.max_tries", 180u32);
            r.assign("o.winrm.timeout", 1800u32);
        }
    });
}

/// The vagrant-vmware-esxi provider: the host from the ESXI_* variables, a port group per NIC.
fn esxi(r: &mut Ruby, g: &Guest) {
    if let Some(b) = g.esxi_box {
        r.assign("o.vm.box", Value::str(b));
    }
    let nets = Value::array(g.port_groups.iter().map(|&i| Value::raw(i.to_string())));
    vagrantfile::esxi_host(r);
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
    if g.nested {
        nested::switch(r, Provider::VmwareEsxi);
    }
    if !g.disks.is_empty() {
        r.assign("v.guest_storage", Value::array(g.disks.iter().map(|(_, gb)| Value::from(*gb))));
    }
}
