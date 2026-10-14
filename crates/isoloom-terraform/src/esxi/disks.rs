//! A machine's data disks on ESXi (`disks`): an `esxi_virtual_disk` each, thin, in a folder of
//! the machine's own on the datastore, attached to its VM by SCSI slot (`virtual_disks`). The
//! machine's `Disks` step finds each one by its size, then formats and mounts it (see
//! `isoloom_render::disks`): the module only makes and attaches them.

use isoloom_plan::{DataDisk, Environment};
use isoloom_render::hcl_builder::{Block, Expression, Module, number, reference, string};

use super::guest::{MAX_DISKS, slotted};
use crate::vm::{res, vms};

/// The resource name of machine `name`'s disk `d`.
fn rid(name: &str, d: &DataDisk) -> String {
    format!("{}_{}", res(name), res(&d.name))
}

/// Machine `name`'s disks, written to `m`; their ids, for its VM to attach in order.
pub fn blocks(name: &str, disks: &[DataDisk], m: &mut Module) -> Vec<Expression> {
    disks
        .iter()
        .map(|d| {
            let id = rid(name, d);
            m.commented(
                &format!("Machine `{name}`'s disk `{}`: {} GB.", d.name, d.size_gb),
                Block::builder("resource")
                    .add_label("esxi_virtual_disk")
                    .add_label(id.as_str())
                    .add_attribute(("virtual_disk_disk_store", reference("var.disk_store")))
                    .add_attribute(("virtual_disk_dir", slotted(&format!("-{name}-disks"))))
                    .add_attribute(("virtual_disk_name", string(format!("{}.vmdk", d.name))))
                    .add_attribute(("virtual_disk_size", number(d.size_gb.into())))
                    .add_attribute(("virtual_disk_type", string("thin")))
                    .build(),
            );
            reference(&format!("esxi_virtual_disk.{id}.id"))
        })
        .collect()
}

/// Why a machine's disks don't fit its VM, if they don't: more than its free SCSI slots.
pub fn refusal(env: &Environment) -> Option<String> {
    vms(env)
        .find(|(_, mp)| mp.disks.len() > MAX_DISKS)
        .map(|(name, _)| format!("machine `{name}`: an ESXi VM takes at most {MAX_DISKS} data disks (SCSI slots 0:1 to 0:15, but 0:7)"))
}
