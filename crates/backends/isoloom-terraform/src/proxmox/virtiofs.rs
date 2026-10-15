//! Shared volumes as folders of the server (`volumes.<name>.virtiofs`, Proxmox VE 8.4 or later):
//! instead of the owner serving its directory over NFS, every VM that mounts the volume gets the
//! server's folder through virtiofs and mounts it by its tag.
//!
//! - The folder is a directory mapping of the cluster. Isoloom creates one per volume,
//!   `iso<slot>-<volume>` for `<virtiofs_root>/<environment>/<volume>` on the node, unless the
//!   volume names a mapping made on the cluster (`mapping`). The folder itself must exist on
//!   the node: Proxmox shares a folder, it doesn't create one.
//! - Each VM gets a `virtiofs` device per volume, with the volume's cache, direct I/O, extended
//!   attributes and ACL settings.
//! - The tag a guest mounts is the mapping's id, which holds the slot: cloud-init writes it to
//!   a file the mount step reads, so the step itself stays the same on every target.

use std::borrow::Cow;

use isoloom_plan::lifecycle::{SharedRole, Step};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, boolean, list, object, reference, string};
use isoloom_spec::model::{MAX_VIRTIOFS, Virtiofs};

use crate::vm::cloud_init::file;
use crate::vm::module::Variable;
use crate::vm::{is_windows, res};

/// Where a VM finds each volume's tag, a file per volume.
pub const TAG_DIR: &str = "/etc/isoloom/virtiofs";

/// The volumes shared as folders of the server, with their settings.
fn volumes(env: &Environment) -> impl Iterator<Item = (&String, &Virtiofs)> {
    env.spec().volumes.iter().filter_map(|(v, sv)| Some((v, sv.virtiofs.as_ref()?)))
}

/// The folders machine `name` mounts, in its own order.
fn mounted_by<'a>(env: &'a Environment, name: &str) -> Vec<(&'a String, &'a Virtiofs)> {
    let Some(m) = env.spec().machines.get(name) else { return Vec::new() };
    volumes(env).filter(|(v, _)| m.volumes.contains_key(*v)).collect()
}

/// A volume's mapping id: the one it names, else the one the module creates.
fn mapping(volume: &str, f: &Virtiofs) -> Expression {
    match &f.mapping {
        Some(id) => string(id.as_str()),
        None => reference(&format!("proxmox_hardware_mapping_dir.{}.name", res(volume))),
    }
}

/// The environment with each VM mounting its folders of the server in place of the NFS steps.
pub fn mounted(env: &Environment) -> Cow<'_, Environment> {
    if volumes(env).next().is_none() {
        return Cow::Borrowed(env);
    }
    let spec = env.spec();
    let mut out = env.clone();
    for (name, mp) in &mut out.machines {
        for step in &mut mp.lifecycle {
            let Step::SharedVolume { volume, role, .. } = step else { continue };
            if spec.volumes.get(volume.as_str()).is_some_and(|v| v.virtiofs.is_some()) {
                *role = SharedRole::HostFolder {
                    tag_file: format!("{TAG_DIR}/{volume}"),
                    folder: spec.machines[name.as_str()].volumes[volume.as_str()].subpath().map(String::from),
                };
            }
        }
    }
    Cow::Owned(out)
}

/// Why a machine can't mount its folders of the server, if one can't: an LXC container (a
/// folder of the server is a bind mount there), a Windows VM (its driver isn't installed), or
/// more folders than a VM takes.
pub fn refusal(env: &Environment) -> Option<String> {
    env.machines.iter().find_map(|(name, mp)| {
        let folders = mounted_by(env, name);
        let (first, _) = folders.first()?;
        if mp.shape == MachineShape::Container {
            Some(format!(
                "machine `{name}`: volume `{first}` is a folder of the server (`virtiofs`), which only a VM mounts: an LXC container would take it as a bind mount (a mount point), which comes later"
            ))
        } else if is_windows(mp) {
            Some(format!(
                "machine `{name}`: volume `{first}` is a folder of the server (`virtiofs`): Windows needs the virtio-fs driver and WinFsp, which the set-up doesn't install"
            ))
        } else if folders.len() > MAX_VIRTIOFS {
            Some(format!(
                "machine `{name}` mounts {} folders of the server (`virtiofs`): a Proxmox VM takes at most {MAX_VIRTIOFS}",
                folders.len()
            ))
        } else {
            None
        }
    })
}

/// Whether the module creates any mapping.
fn creates(env: &Environment) -> bool {
    volumes(env).any(|(_, f)| f.mapping.is_none())
}

/// The folder the created mappings point under, when the module creates any.
pub fn variables(env: &Environment) -> Vec<Block> {
    if !creates(env) {
        return Vec::new();
    }
    vec![
        Variable::new("virtiofs_root", "string")
            .default(string("/srv/isoloom"))
            .describe("On the node, the folder holding each virtiofs volume's own: <root>/<environment>/<volume>, made before apply")
            .block(),
    ]
}

/// A directory mapping per volume that names none: the folder on the node.
pub fn mappings(env: &Environment, m: &mut Module) {
    for (v, _) in volumes(env).filter(|(_, f)| f.mapping.is_none()) {
        let path = Template::new()
            .with_expr(reference("var.virtiofs_root"))
            .with_text(format!("/{}/{v}", env.name))
            .into_expression();
        let block = Block::builder("resource")
            .add_label("proxmox_hardware_mapping_dir")
            .add_label(res(v))
            .add_attribute((
                "name",
                Template::text("iso")
                    .with_expr(reference("var.slot"))
                    .with_text(format!("-{v}"))
                    .into_expression(),
            ))
            .add_attribute(("comment", string(format!("Isoloom: {}, volume {v}", env.name))))
            .add_attribute(("map", list([object([("node", reference("var.node")), ("path", path)])])))
            .build();
        m.commented(
            &format!("Volume `{v}`: a folder of the server, shared into each VM that mounts it (virtiofs). Make the folder on the node first."),
            block,
        );
    }
}

/// Machine `name`'s `virtiofs` devices, one per folder it mounts.
pub fn devices(env: &Environment, name: &str) -> Vec<Block> {
    mounted_by(env, name)
        .into_iter()
        .map(|(v, f)| {
            Block::builder("virtiofs")
                .add_attribute(("mapping", mapping(v, f)))
                .add_attribute(("cache", string(f.cache.unwrap_or_default().id())))
                .add_attribute(("direct_io", boolean(f.direct_io)))
                .add_attribute(("expose_xattr", boolean(f.exposes_xattr())))
                .add_attribute(("expose_acl", boolean(f.acl)))
                .build()
        })
        .collect()
}

/// Machine `name`'s tag files, written by cloud-init: each folder's mapping id.
pub fn tag_files(env: &Environment, name: &str) -> Vec<Expression> {
    mounted_by(env, name)
        .into_iter()
        .map(|(v, f)| file(&format!("{TAG_DIR}/{v}"), Some("0644"), mapping(v, f)))
        .collect()
}
