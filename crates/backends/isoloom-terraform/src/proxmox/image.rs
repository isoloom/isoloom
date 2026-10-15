//! A machine's own Proxmox image (`vm.image.proxmox`): a cloud image's URL, downloaded for the
//! machine, or the id of a Linux template VM on the node, cloned with its disk grown. A Windows
//! machine boots its OS's template (`windows`), so its own image is refused.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, boolean, reference, string};

use super::machine::Boot;
use crate::vm::{res, vms};

/// A machine's own Proxmox image.
enum Own<'a> {
    /// A cloud image to download.
    Url(&'a str),
    /// A template VM's id.
    Template(u32),
}

fn own(mp: &MachinePlan) -> Option<Own<'_>> {
    let x = mp.vm_image.as_ref()?.proxmox.as_deref()?;
    Some(x.parse().map_or(Own::Url(x), Own::Template))
}

/// The resource name of machine `name`'s own cloud image.
fn own_res(name: &str) -> String {
    format!("{}_image", res(name))
}

/// Whether the machine boots an image of its own (so its OS needs no image of Isoloom's).
pub fn has_own(mp: &MachinePlan) -> bool {
    own(mp).is_some()
}

/// What machine `name` boots when it has an image of its own.
pub fn boot(name: &str, mp: &MachinePlan) -> Option<Boot> {
    Some(match own(mp)? {
        Own::Url(_) => Boot::Image(own_res(name)),
        Own::Template(id) => Boot::Clone(id),
    })
}

/// A full clone of the template VM `vm_id`.
pub fn clone(vm_id: Expression) -> Block {
    Block::builder("clone")
        .add_attribute(("vm_id", vm_id))
        .add_attribute(("full", boolean(true)))
        .build()
}

/// Why a machine's own image can't be used, if it can't: a Windows machine boots its OS's
/// template.
pub fn refusal(env: &Environment) -> Option<String> {
    vms(env)
        .find(|(_, mp)| mp.is_windows() && has_own(mp))
        .map(|(name, _)| format!("machine `{name}` sets `vm.image.proxmox`: a Windows machine boots its OS's template (the module's `template_<os>` variable)"))
}

/// One download per machine with a cloud image of its own, named after the machine.
pub fn downloads(env: &Environment, m: &mut Module) {
    for (name, mp) in vms(env) {
        let Some(Own::Url(url)) = own(mp) else { continue };
        let block = Block::builder("resource")
            .add_label("proxmox_download_file")
            .add_label(own_res(name))
            .add_attribute(("node_name", reference("var.node")))
            .add_attribute(("datastore_id", reference("var.image_datastore")))
            .add_attribute(("content_type", string("iso")))
            .add_attribute(("url", string(url)))
            .add_attribute((
                "file_name",
                Template::text("iso")
                    .with_expr(reference("var.slot"))
                    .with_text(format!("-{name}.img"))
                    .into_expression(),
            ))
            .add_attribute(("overwrite_unmanaged", boolean(true)))
            .build();
        m.commented(&format!("Machine `{name}`'s own cloud image (`vm.image.proxmox`)."), block);
    }
}
