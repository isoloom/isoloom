//! Why a lab isn't generated for a target: not possible by its machines' forms, refused by the
//! plan or by the target's backend, or (on `cloud-vm`) a cloud whose module is dropped. These
//! answer what `isoloom targets` lists and the snapshot's `targets` section holds.

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;

use crate::registry::backend;

/// A cloud refused by a multi-cloud target: the cloud's folder under the target's, and why it
/// can't run the environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CloudRefusal {
    /// The cloud's id, its folder under the target's (`gcp`).
    pub cloud: &'static str,
    /// Why it can't run the environment.
    pub reason: String,
}

/// The clouds of a multi-cloud target: those whose module is written and those refused.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Clouds {
    /// The clouds whose module is written.
    pub generated: Vec<&'static str>,
    /// The clouds whose module is dropped, each with its reason.
    pub refused: Vec<CloudRefusal>,
}

/// Why `target`'s generator would refuse this lab, if it would: the thing it doesn't support
/// yet (a Windows machine on Proxmox, environment-level provisioning on Docker, ...). `None`
/// when `generate` produces the target. Lets `isoloom targets` tell the truth: a target can be
/// possible by its machines' forms and still not be generated.
///
/// Safe on a lab that hasn't been checked: an invalid lab is refused by every target, with its
/// first problem as the reason, instead of reaching a generator that assumes validity.
#[must_use]
pub fn refusal(lab: &Lab, target: Target) -> Option<String> {
    if let Some(problem) = lab.problems().first() {
        return Some(format!("the spec is invalid: {}", problem.line()));
    }
    if let Some(refused) = lab.refusal(target) {
        return Some(refused.line());
    }
    if !lab.possible().contains(&target) {
        return None;
    }
    let spec = lab.for_target(target);
    if let Some(p) = isoloom_spec::validate::validate(&spec).first() {
        return Some(format!("the spec is invalid on {}: {p}", target.id()));
    }
    backend(target)?.refusal(&Environment::build(&spec, target, None))
}

/// [`refusal`], for a target run with a given provider: why `target` can't run this lab when
/// its tool uses `provider`, if it can't. `provider` is the name the target's tool knows it by,
/// as `defaults.yml` sets it: for Vagrant, `vagrant up --provider` (`virtualbox`, `libvirt`,
/// `qemu`, ...). `None` is the target whatever the provider, which is [`refusal`].
///
/// Today `vagrant` refuses more on some providers: a machine requiring a TPM (`hardware.tpm:
/// required`) anywhere but `virtualbox`, `libvirt` and `hyperv`; on `qemu`, a VM is on one
/// network (QEMU gives it one); on `utm`, a VM is on none (`vagrant_utm` has no private
/// networks); on `hyperv`, a machine whose OS has no known Hyper-V box needs a box override;
/// `qemu`, `hyperv` and `libvirt` have no bridged networks yet; a machine booting its own way
/// (`hardware.boot`) runs on `libvirt` (on `qemu` with UEFI and `firmware_format` only).
#[must_use]
pub fn refusal_on(lab: &Lab, target: Target, provider: Option<&str>) -> Option<String> {
    refusal(lab, target).or_else(|| match (target, provider) {
        (Target::Vagrant, Some(p)) => {
            let spec = lab.for_target(target);
            isoloom_vagrant::provider_refusal(&spec, p).or_else(|| isoloom_vagrant::disk_refusal(&spec, p))
        }
        _ => None,
    })
}

/// The clouds of a target that writes one module per cloud (`cloud-vm`), when the target is
/// generated. A cloud refusing drops only its module, so the target is generated as long as one
/// cloud accepts the environment. `None` for every other target, and for a `cloud-vm` that isn't
/// generated (its refusal says why).
#[must_use]
pub fn cloud_refusals(lab: &Lab, target: Target) -> Option<Clouds> {
    if target != Target::CloudVm || !lab.possible().contains(&target) || refusal(lab, target).is_some() {
        return None;
    }
    let env = Environment::build(&lab.for_target(target), target, None);
    let mut clouds = Clouds::default();
    for (cloud, why) in isoloom_terraform::CloudVmBackend.refusals(&env) {
        match why {
            None => clouds.generated.push(cloud.id()),
            Some(reason) => clouds.refused.push(CloudRefusal { cloud: cloud.id(), reason }),
        }
    }
    Some(clouds)
}

/// Why a target isn't possible for the lab: the first machine it has no form for, at the field
/// that rules it out.
#[must_use]
pub fn not_possible_reason(lab: &Lab, t: Target) -> String {
    if let Some(missing) = isoloom_plan::lower::missing_form(lab.source(), lab.catalogue(), t) {
        missing.line()
    } else if t == Target::Hybrid && !isoloom_spec::targets::mixed(&lab.spec()) {
        "only when some machines are containers and others are VMs".to_owned()
    } else {
        "left out by `targets:`".to_owned()
    }
}
