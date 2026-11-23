//! The vagrant-vmware-esxi provider: the host from the `ESXI_*` variables, a port group per NIC.

use super::nested;
use super::provider::Provider;
use super::providers::{self, Guest};
use crate::ruby::{Ruby, Value};
use crate::vagrantfile;

/// The ESXi provider's block; with its override (`o`) when the VM boots a box of its own there.
pub(super) fn block(r: &mut Ruby, g: &Guest) {
    let params = if g.esxi_box.is_some() { "v, o" } else { "v" };
    r.block(providers::provider(Provider::VmwareEsxi), params, |r| settings(r, g));
}

fn settings(r: &mut Ruby, g: &Guest) {
    if let Some(b) = g.esxi_box {
        r.assign("o.vm.box", Value::str(b));
    }
    let nets = Value::array(g.port_groups.iter().map(|&i| Value::raw(i.to_string())));
    vagrantfile::esxi_host(r);
    r.assign("v.esxi_virtual_network", Value::raw(format!("esxi_networks({nets})")));
    r.assign("v.guest_name", crate::vagrantfile::host_name(&g.vm_name));
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
