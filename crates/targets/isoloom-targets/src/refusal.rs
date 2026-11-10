//! Why a lab isn't generated for a target: not possible by its machines' forms, refused by the
//! plan or by the target's backend, or (on `cloud-vm`) a cloud whose module is dropped. These
//! answer what `isoloom targets` lists and the snapshot's `targets` section holds, from the
//! decisions the capability matrix shows ([`crate::decision`]).

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;

use crate::decision::{Decision, decide};

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

/// Why `target`'s generator would refuse this lab, if it would: a field lowering rules out, a
/// capability its backend lacks, a limit of its own (a Windows machine's name too long on
/// Proxmox, environment-level provisioning on Docker, ...). `None` when `generate` produces the
/// target, and when the target isn't possible for the lab at all ([`not_possible_reason`] says
/// why). Lets `isoloom targets` tell the truth: a target can be possible by its machines' forms
/// and still not be generated.
///
/// Safe on a lab that hasn't been checked: an invalid lab is refused by every target, with its
/// first problem as the reason, instead of reaching a generator that assumes validity.
#[must_use]
pub fn refusal(lab: &Lab, target: Target) -> Option<String> {
    refusal_on(lab, target, None)
}

/// [`refusal`], for a target run with a given provider: why `target` can't run this lab when
/// its tool uses `provider`, if it can't. `provider` is the name the target's tool knows it by,
/// as `defaults.yml` sets it: for Vagrant, `vagrant up --provider` (`virtualbox`, `libvirt`,
/// `qemu`, ...). `None` is the target whatever the provider, which is [`refusal`].
///
/// On `vagrant`, a provider refuses what [`isoloom_vagrant::provider_refusal`] says; on
/// `docker-vm`, what [`isoloom_vagrant::docker_vm_refusal`] says of a provider, and the Proxmox
/// server (`proxmox`) a container's host folders (its Proxmox module isn't written then); on
/// `cloud-vm`, a cloud (`aws`) what its module refuses.
#[must_use]
pub fn refusal_on(lab: &Lab, target: Target, provider: Option<&str>) -> Option<String> {
    match decide(lab, target, provider) {
        Decision::Refused(r) => Some(r.line()),
        Decision::Generated | Decision::NotApplicable => None,
    }
}

/// The clouds of a target that writes one module per cloud (`cloud-vm`, `cloud-docker`), when the
/// target is generated. A cloud refusing drops only its module, so the target is generated as
/// long as one cloud accepts the environment. `None` for every other target, and for one that
/// isn't generated (its refusal says why).
#[must_use]
pub fn cloud_refusals(lab: &Lab, target: Target) -> Option<Clouds> {
    if !matches!(target, Target::CloudVm | Target::CloudDocker) || !lab.possible().contains(&target) || refusal(lab, target).is_some() {
        return None;
    }
    let env = Environment::build(&lab.for_target(target), target, None);
    let each = if target == Target::CloudVm {
        isoloom_terraform::CloudVmBackend.refusals(&env)
    } else {
        isoloom_terraform::cloud_docker_refusals(&env)
    };
    let mut clouds = Clouds::default();
    for (cloud, why) in each {
        match why {
            None => clouds.generated.push(cloud.id()),
            Some(reason) => clouds.refused.push(CloudRefusal { cloud: cloud.id(), reason }),
        }
    }
    Some(clouds)
}

/// Why a target isn't possible for the lab: cloud services on a machine target, machines on
/// `cloud-services`, the first machine it has no form for (at the field that rules it out),
/// `hybrid` without both containers and VMs, or `targets:` leaving it out.
#[must_use]
pub fn not_possible_reason(lab: &Lab, t: Target) -> String {
    let spec = lab.source();
    if spec.cloud.is_some() && t != Target::CloudServices {
        "cloud services run on `cloud-services` only".to_owned()
    } else if spec.cloud.is_none() && t == Target::CloudServices {
        "needs `cloud:` (cloud services instead of machines)".to_owned()
    } else if spec.machines.is_empty() {
        "the spec has no machines".to_owned()
    } else if let Some(missing) = lab.missing_form(t) {
        missing.line()
    } else if t == Target::Hybrid && !isoloom_spec::targets::mixed(&lab.spec()) {
        "only when some machines are containers and others are VMs".to_owned()
    } else if spec.targets.as_ref().is_some_and(|wanted| !wanted.contains(&t)) {
        "left out by `targets:`".to_owned()
    } else {
        // `Lab::possible` rules a target out for one of the reasons above only.
        "not possible for this spec".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab(yaml: &str) -> Lab {
        Lab::new(isoloom_spec::parse(yaml).unwrap())
    }

    const MACHINES: &str = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { os: debian@12, networks: { lan: 10 }, app: { image: redis, steps: [{ run: a.sh }] } }\n";

    #[test]
    fn a_target_not_possible_says_why_and_targets_only_when_the_spec_has_targets() {
        let cloud = lab("version: 1\nname: t\ncloud: { provider: aws, terraform: infra }\n");
        assert_eq!(not_possible_reason(&cloud, Target::Docker), "cloud services run on `cloud-services` only");
        let machines = lab(MACHINES);
        assert_eq!(
            not_possible_reason(&machines, Target::CloudServices),
            "needs `cloud:` (cloud services instead of machines)"
        );
        assert_eq!(
            not_possible_reason(&machines, Target::Hybrid),
            "only when some machines are containers and others are VMs"
        );
        let narrowed = lab(&format!("{MACHINES}targets: [docker]\n"));
        assert!(!narrowed.possible().contains(&Target::Vagrant));
        assert_eq!(not_possible_reason(&narrowed, Target::Vagrant), "left out by `targets:`");
        for t in Target::ALL.into_iter().filter(|t| !machines.possible().contains(t)) {
            assert_ne!(not_possible_reason(&machines, t), "left out by `targets:`", "{}", t.id());
        }
    }

    #[test]
    fn a_refusal_reads_as_the_decision_the_matrix_shows() {
        let image = lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: redis } }\n");
        let why = refusal(&image, Target::Vagrant).unwrap();
        assert!(why.starts_with("machines.a.app.image: "), "{why}");
        let invalid =
            lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10, wan: 3 }, app: { image: redis } }\n");
        let invalid_why = refusal(&invalid, Target::Docker).unwrap();
        assert!(invalid_why.starts_with("the spec is invalid: machines.a.networks"), "{invalid_why}");
        assert_eq!(refusal(&lab(MACHINES), Target::Docker), None);
        assert_eq!(refusal(&lab(MACHINES), Target::CloudServices), None, "not possible, not refused");
        let two = lab(&format!("{MACHINES}  b: {{ os: debian@12, networks: {{ lan: 11 }} }}\n"));
        assert_eq!(refusal(&two, Target::CloudVm), None);
        let digitalocean = refusal_on(&two, Target::CloudVm, Some("digitalocean")).unwrap_or_default();
        assert!(digitalocean.contains("one machine on one network"), "{digitalocean}");
    }
}
