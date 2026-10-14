//! A machine's data disks on each cloud: a block volume of its size, attached to its instance
//! (at its creation where the cloud can: AWS, Google Cloud, DigitalOcean, Oracle Cloud).
//! The machine's `Disks` step finds each attached disk by its size and formats and mounts it
//! (see `isoloom_render::disks`), so the resources here only create and attach them. A cloud's
//! smallest volume is a refusal of its own ([`too_small`]).

use isoloom_plan::{DataDisk, Environment};
use isoloom_render::hcl_builder::{Block, Expression, boolean, call, list, number, reference, string};

use super::cloud::{name_with, resource};
use super::{res, vms};

/// The resource name of machine `id`'s disk `d`.
fn rid(id: &str, d: &DataDisk) -> String {
    format!("{id}_{}", res(&d.name))
}

/// The cloud name of machine `name`'s disk `d`: `<environment>-<machine>-<disk>`.
fn cloud_name(name: &str, d: &DataDisk) -> Expression {
    name_with(&format!("-{name}-{}", d.name))
}

/// AWS: an EBS volume per disk, created with the instance and attached at `/dev/sdf` on (the
/// instance's `ebs_block_device` blocks): there at first boot, gone with the instance.
pub fn aws(disks: &[DataDisk]) -> Vec<Block> {
    disks
        .iter()
        .zip('f'..='p')
        .map(|(d, letter)| {
            Block::builder("ebs_block_device")
                .add_attribute(("device_name", string(format!("/dev/sd{letter}"))))
                .add_attribute(("volume_size", number(d.size_gb.into())))
                .add_attribute(("volume_type", string("gp3")))
                .add_attribute(("delete_on_termination", boolean(true)))
                .build()
        })
        .collect()
}

/// Azure: an empty managed disk in the environment's resource group, attached at LUN 0 on to
/// the VM of resource type `vm` (Linux or Windows).
pub fn azure(name: &str, vm: &str, disks: &[DataDisk]) -> Vec<Block> {
    let id = res(name);
    disks
        .iter()
        .enumerate()
        .flat_map(|(lun, d)| {
            let r = rid(&id, d);
            [
                resource("azurerm_managed_disk", &r)
                    .add_attribute(("name", cloud_name(name, d)))
                    .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
                    .add_attribute(("location", reference("var.region")))
                    .add_attribute(("storage_account_type", string("StandardSSD_LRS")))
                    .add_attribute(("create_option", string("Empty")))
                    .add_attribute(("disk_size_gb", number(d.size_gb.into())))
                    .add_attribute(("tags", reference("local.tags")))
                    .build(),
                resource("azurerm_virtual_machine_data_disk_attachment", &r)
                    .add_attribute(("managed_disk_id", reference(&format!("azurerm_managed_disk.{r}.id"))))
                    .add_attribute(("virtual_machine_id", reference(&format!("{vm}.{id}.id"))))
                    .add_attribute(("lun", number(u64::try_from(lun).unwrap_or(u64::MAX))))
                    .add_attribute(("caching", string("ReadWrite")))
                    .build(),
            ]
        })
        .collect()
}

/// Google Cloud: a balanced persistent disk per disk in the module's zone (the instance's
/// `attached_disk` blocks attach them, see [`gcp_attached`]).
pub fn gcp(name: &str, disks: &[DataDisk]) -> Vec<Block> {
    let id = res(name);
    disks
        .iter()
        .map(|d| {
            resource("google_compute_disk", &rid(&id, d))
                .add_attribute(("project", reference("local.project")))
                .add_attribute(("name", cloud_name(name, d)))
                .add_attribute(("type", string("pd-balanced")))
                .add_attribute(("zone", reference("local.zone")))
                .add_attribute(("size", number(d.size_gb.into())))
                .build()
        })
        .collect()
}

/// Google Cloud: the instance's `attached_disk` blocks, each of its disks by its name.
pub fn gcp_attached(name: &str, disks: &[DataDisk]) -> Vec<Block> {
    let id = res(name);
    disks
        .iter()
        .map(|d| {
            Block::builder("attached_disk")
                .add_attribute(("source", reference(&format!("google_compute_disk.{}.id", rid(&id, d)))))
                .add_attribute(("device_name", string(d.name.as_str())))
                .build()
        })
        .collect()
}

/// DigitalOcean: a volume per disk in the module's region (the droplet's `volume_ids` attach
/// them, see [`digitalocean_ids`]).
pub fn digitalocean(name: &str, disks: &[DataDisk]) -> Vec<Block> {
    let id = res(name);
    disks
        .iter()
        .map(|d| {
            resource("digitalocean_volume", &rid(&id, d))
                .add_attribute(("name", cloud_name(name, d)))
                .add_attribute(("region", reference("var.region")))
                .add_attribute(("size", number(d.size_gb.into())))
                .build()
        })
        .collect()
}

/// The droplet's `volume_ids`: its disks' volumes, attached at creation.
pub fn digitalocean_ids(name: &str, disks: &[DataDisk]) -> Expression {
    let id = res(name);
    list(disks.iter().map(|d| reference(&format!("digitalocean_volume.{}.id", rid(&id, d)))))
}

/// Linode: a volume in the module's region, attached to the instance. Its label is at most 32
/// characters: the environment's id, shortened, then the machine and the disk.
pub fn linode(name: &str, disks: &[DataDisk]) -> Vec<Block> {
    let id = res(name);
    disks
        .iter()
        .map(|d| {
            let short = call("substr", [call("md5", [reference("local.name")]), number(0), number(8)]);
            let label = call("format", [string(format!("v%s-{name}-{}", d.name)), short]);
            resource("linode_volume", &rid(&id, d))
                .add_attribute(("label", call("substr", [label, number(0), number(32)])))
                .add_attribute(("region", reference("var.region")))
                .add_attribute(("size", number(d.size_gb.into())))
                .add_attribute(("linode_id", reference(&format!("linode_instance.{id}.id"))))
                .build()
        })
        .collect()
}

/// Oracle Cloud: a block volume per disk, created with the instance and attached paravirtualized
/// (no iSCSI commands to run in the machine): its `launch_volume_attachments` blocks.
pub fn oci(name: &str, disks: &[DataDisk]) -> Vec<Block> {
    disks
        .iter()
        .map(|d| {
            let create = Block::builder("launch_create_volume_details")
                .add_attribute(("volume_creation_type", string("ATTRIBUTES")))
                .add_attribute(("compartment_id", reference("var.compartment_id")))
                .add_attribute(("display_name", cloud_name(name, d)))
                .add_attribute(("size_in_gbs", number(d.size_gb.into())))
                .build();
            Block::builder("launch_volume_attachments")
                .add_attribute(("type", string("paravirtualized")))
                .add_attribute(("display_name", string(d.name.as_str())))
                .add_block(create)
                .build()
        })
        .collect()
}

/// The most data disks a machine takes on AWS: one attachment letter each, `f` to `p`.
pub const AWS_MAX_DISKS: usize = 11;

/// Why `cloud` can't give a VM of `env` its disks, when one is smaller than the cloud's
/// smallest volume (`min_gb`), or a machine has more than `max` of them.
pub fn too_small(env: &Environment, cloud: &str, min_gb: u32, max: usize) -> Option<String> {
    vms(env).find_map(|(name, mp)| {
        if mp.disks.len() > max {
            return Some(format!("machine `{name}`: {cloud} attaches at most {max} data disks to a VM"));
        }
        let d = mp.disks.iter().find(|d| d.size_gb < min_gb)?;
        Some(format!("machine `{name}`, disk `{}`: {cloud}'s volumes are {min_gb} GB at least", d.name))
    })
}
