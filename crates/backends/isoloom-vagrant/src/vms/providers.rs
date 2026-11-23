//! Every provider's block for one VM: the same set for the environment's machines and Isoloom's own
//! (router, controller, tool shell), so a provider added here reaches all of them. Each provider's
//! block, and what it alone works around, is in that provider's module ([`vmware`], [`qemu`],
//! [`libvirt`], [`esxi`], [`hyperv`], [`utm`]); VirtualBox and Parallels share [`desktop`].

use crate::console;
use isoloom_spec::model::Boot;
use isoloom_spec::windows::Tpm;

use super::hyperv::{self, HyperV};
use super::provider::Provider;
use super::qemu::{self, Qemu};
use super::utm::{self, Utm};
use super::{esxi, libvirt, nested, vmware, windows};
use crate::ruby::{Call, Ruby, Value};

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

/// One VM, as its providers see it.
#[derive(Debug, Clone)]
pub(crate) struct Guest<'a> {
    /// Its display name.
    pub(crate) label: isoloom_plan::HostName,
    /// Its name on ESXi and Hyper-V (`<env>-<key>-<machine>`).
    pub(crate) vm_name: isoloom_plan::HostName,
    /// Its size.
    pub(crate) size: Size,
    /// How many of the environment's networks it is on: its adapters after the box's own, in the
    /// order of its `private_network` lines.
    pub(crate) networks: usize,
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
    /// How UTM runs it.
    pub(crate) utm: Utm<'a>,
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

/// The provider blocks of `g`. Its disk size grows the box's disk, never below its own: Vagrant's
/// own disk setting in VirtualBox's and VMware's blocks (each grows a disk and leaves a bigger one
/// as it is), Hyper-V's step before the VM starts ([`hyperv`]: Vagrant's setting would shrink a
/// VHDX there, which Hyper-V refuses), QEMU's, libvirt's and ESXi's in their blocks.
pub(crate) fn blocks(r: &mut Ruby, g: &Guest) {
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
        console::virtualbox_display(r);
        if g.tpm.is_some() {
            windows::virtualbox_tpm(r);
        }
        if g.console.is_some() {
            console::virtualbox(r);
        }
    });
    vmware::block(r, g);
    desktop(r, Provider::Parallels, g, |_| {});
    qemu::block(r, g);
    esxi::block(r, g);
    libvirt::block(r, g);
    hyperv::block(r, &g.vm_name, g.size, &g.hyperv);
    utm::block(r, &g.label, &g.vm_name, g.size, &g.utm);
    if g.nested {
        nested::refuse_elsewhere(r);
    }
}

/// `m.vm.provider "<id>"`: the head of a provider's block.
pub(super) fn provider(p: Provider) -> Call {
    Call::new("m.vm.provider").arg(Value::str(p.id()))
}

/// Vagrant's own setting for the box's disk at `size`'s, in a provider's block (`o`, its
/// override): VirtualBox and VMware grow a disk to it and leave a bigger one as it is.
pub(super) fn primary_disk(r: &mut Ruby, size: Size) {
    if let Some(d) = size.disk {
        r.call(
            Call::new("o.vm.disk")
                .arg(Value::sym("disk"))
                .kw("size", Value::str(&format!("{d}GB")))
                .kw("primary", true),
        );
    }
}

/// VirtualBox and Parallels: a name, CPUs, memory and linked clones, then what only that
/// provider sets (`more`).
fn desktop(r: &mut Ruby, p: Provider, g: &Guest, more: impl FnOnce(&mut Ruby)) {
    r.block(provider(p), "v, o", |r| {
        if p == Provider::VirtualBox {
            primary_disk(r, g.size);
        }
        r.assign("v.name", crate::vagrantfile::host_name(&g.label));
        r.assign("v.linked_clone", true);
        r.assign("v.cpus", g.size.cpus);
        r.assign("v.memory", g.size.mem);
        more(r);
        if g.nested {
            nested::switch(r, p);
        }
    });
}
