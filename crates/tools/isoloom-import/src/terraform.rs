//! Drafts an `isoloom.yml` from Terraform configuration (the `.tf` files of one folder), read
//! statically: `variable` defaults and `locals` are evaluated, `for_each` and `count` expanded,
//! and references between resources followed (a VM to its network interface to its subnet).
//! VMs and subnets of AWS, Azure, Google Cloud and Proxmox (bpg) become machines and networks;
//! every other resource type is left out, with a note.

mod eval;
mod instance;
mod placement;
mod subnets;
mod vms;

use std::collections::BTreeSet;

use crate::draft::{Draft, LEFT_OUT, NoteKind, Notes};
use crate::naming::label;
use crate::{Error, Result};
use eval::instances;
use instance::Instance;
use placement::Placement;
use vms::Machine;

/// Drafts a spec from the `.tf` files of a folder (`files`: name and text). `fallback_name`
/// names the environment (usually its folder); `source` is how the files are named in the
/// draft's header.
///
/// # Errors
///
/// [`Error::Hcl`] when a file isn't valid HCL, and [`Error::NoTerraformVm`] when the files
/// declare no VM resource the import reads.
pub fn draft(files: &[(String, String)], fallback_name: &str, source: &str) -> Result<Draft> {
    let mut notes = Notes::default();
    let mut bodies = Vec::new();
    for (name, text) in files {
        bodies.push(hcl::parse(text).map_err(|e| Error::Hcl { file: name.clone(), source: e })?);
    }
    let all = instances(&bodies, &mut notes);
    let nets = subnets::networks(&all, &mut notes);
    let machines: Vec<Machine> = all.iter().filter_map(|i| vms::machine(i, &all, &mut notes)).collect();
    if machines.is_empty() {
        return Err(Error::NoTerraformVm);
    }
    other_resources(&all, &mut notes);
    let placement = Placement::new(&machines, &nets);
    Ok(notes.draft(placement.write(&machines, &label(fallback_name), source)))
}

/// Every resource type the import doesn't read as a machine or a network, once each: left out.
fn other_resources(all: &[Instance], notes: &mut Notes) {
    let mut seen = BTreeSet::new();
    for i in all {
        if vms::reads(&i.kind) || subnets::reads(i) || !seen.insert(i.kind.clone()) {
            continue;
        }
        notes.add(format!("resource {}", i.kind), NoteKind::Changed, LEFT_OUT);
    }
}
