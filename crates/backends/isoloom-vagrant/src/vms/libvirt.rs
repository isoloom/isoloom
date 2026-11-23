//! The vagrant-libvirt provider.

use super::provider::Provider;
use super::providers::{self, Guest};
use super::{boot, nested, qemu, windows};
use crate::console;
use crate::ruby::{Call, Ruby, Value};

/// libvirt runs an environment machine as its own CPU (emulated when the host's is another), as QEMU
/// does; Isoloom's own VMs take the host's.
pub(super) fn block(r: &mut Ruby, g: &Guest) {
    let params = if g.libvirt_box.is_some() { "v, o" } else { "v" };
    r.block(providers::provider(Provider::Libvirt), params, |r| {
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
            r.assign("v.machine_arch", Value::str(qemu::arch(a)));
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
