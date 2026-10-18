//! What a target decides about a lab, typed: it generates it, or it refuses it and at which step
//! (lowering, the backend's capabilities, the backend's own limits, a Vagrant provider's, a
//! cloud's), or the lab isn't one the target takes at all. The same decisions as [`refusal`],
//! [`refusal_on`] and [`cloud_refusals`], per [`Column`], so the capability matrix
//! ([`crate::matrix`]) is what the generators decide and nothing more.
//!
//! [`refusal`]: crate::refusal
//! [`refusal_on`]: crate::refusal_on
//! [`cloud_refusals`]: crate::cloud_refusals

use isoloom_plan::{Environment, Feature, Lab};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};

use crate::registry::backend;
use crate::tiers::VAGRANT_PROVIDERS;

/// A target, or one of its Vagrant providers or clouds where each decides on its own: a column
/// of the capability matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Column {
    /// The target.
    pub target: Target,
    /// The Vagrant provider (`libvirt`) or the cloud (`aws`), for `vagrant` and `cloud-vm`.
    pub variant: Option<&'static str>,
}

impl Column {
    /// Its name: the target's id, then `/` and the provider or cloud (`vagrant/libvirt`).
    #[must_use]
    pub fn id(&self) -> String {
        crate::tiers::name(self.target, self.variant)
    }
}

/// Every column, in the order targets are listed: `vagrant` once per provider, `cloud-vm` once
/// per cloud, every other target once.
#[must_use]
pub fn columns() -> Vec<Column> {
    let of = |target, variant| Column { target, variant };
    Target::ALL
        .into_iter()
        .flat_map(|t| match t {
            Target::Vagrant => VAGRANT_PROVIDERS.iter().map(|p| of(t, Some(*p))).collect(),
            Target::CloudVm => Cloud::ALL.iter().map(|c| of(t, Some(c.id()))).collect(),
            _ => vec![of(t, None)],
        })
        .collect()
}

/// The step that refuses a lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Code {
    /// The spec isn't valid, or not once lowered for the target.
    Invalid,
    /// Lowering: a machine has no form the target runs, or a step the target doesn't run yet,
    /// at the field that rules it out.
    Lowering,
    /// The target's backend lacks the capability the lab needs.
    Capability(Feature),
    /// A limit of the target's own (its images, its platform's rules).
    Target,
    /// A limit of the Vagrant provider's own.
    Provider,
    /// A limit of the cloud's own.
    Cloud,
}

impl Code {
    /// The code as the matrix writes it: `invalid`, `lowering`, `capability`, `target`,
    /// `provider` or `cloud`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Code::Invalid => "invalid",
            Code::Lowering => "lowering",
            Code::Capability(_) => "capability",
            Code::Target => "target",
            Code::Provider => "provider",
            Code::Cloud => "cloud",
        }
    }

    /// The capability lacking, for [`Code::Capability`].
    #[must_use]
    pub fn feature(self) -> Option<Feature> {
        match self {
            Code::Capability(f) => Some(f),
            _ => None,
        }
    }
}

/// A refusal: the step, the field it is about when that step names one, and the reason as the
/// generator gives it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Refused {
    /// The step that refuses.
    pub code: Code,
    /// The field, as written in the spec (`machines.web.app.image`), when the step names one.
    pub field: Option<String>,
    /// Why, as the generator says it.
    pub reason: String,
}

/// What a column decides about a lab.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Decision {
    /// It generates the lab.
    Generated,
    /// It refuses it.
    Refused(Refused),
    /// The lab isn't one the target takes: cloud services on a machine target, machines on
    /// `cloud-services`, `hybrid` without both containers and VMs, a target `targets:` leaves
    /// out.
    NotApplicable,
}

/// A refusal at `code`.
fn refused(code: Code, field: Option<String>, reason: impl Into<String>) -> Decision {
    Decision::Refused(Refused {
        code,
        field,
        reason: reason.into(),
    })
}

/// What `column` decides about `lab`: the spec's own problems, lowering, then the backend's
/// capabilities and its own limits (a cloud's, for a `cloud-vm` column), then the Vagrant
/// provider's.
#[must_use]
pub fn decide(lab: &Lab, column: &Column) -> Decision {
    let target = column.target;
    if let Some(p) = lab.problems().into_iter().next() {
        return refused(Code::Invalid, Some(p.path.to_string()), p.reason);
    }
    if let Some(p) = lab.refusal(target) {
        return refused(Code::Lowering, Some(p.path.to_string()), p.reason);
    }
    if !lab.possible().contains(&target) {
        return Decision::NotApplicable;
    }
    let spec = lab.for_target(target);
    if let Some(p) = isoloom_spec::validate::validate(&spec).first() {
        return refused(Code::Invalid, None, format!("the spec is invalid on {}: {p}", target.id()));
    }
    let Some(backend) = backend(target) else {
        return refused(Code::Target, None, "no generator yet");
    };
    let env = Environment::build(&spec, target, None);
    let lacking = isoloom_plan::refusal(&env.requirements, &backend.capabilities());
    let own = match (target, column.variant.and_then(cloud)) {
        (Target::CloudVm, Some(c)) => CloudVmModule::new(c).refusal(&env).map(|r| (Code::Cloud, r)),
        _ => backend.refusal(&env).map(|r| (Code::Target, r)),
    };
    match (lacking, own) {
        (Some(lack), Some((_, reason))) => refused(Code::Capability(lack.requirement.feature()), None, reason),
        (Some(lack), None) => refused(Code::Capability(lack.requirement.feature()), None, lack.to_string()),
        (None, Some((code, reason))) => refused(code, None, reason),
        (None, None) => match (target, column.variant) {
            (Target::Vagrant, Some(p)) => isoloom_vagrant::provider_refusal(&spec, p)
                .or_else(|| isoloom_vagrant::disk_refusal(&spec, p))
                .map_or(Decision::Generated, |r| refused(Code::Provider, None, r)),
            _ => Decision::Generated,
        },
    }
}

/// The cloud with this id.
fn cloud(id: &str) -> Option<Cloud> {
    Cloud::ALL.into_iter().find(|c| c.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab(yaml: &str) -> Lab {
        Lab::new(isoloom_spec::parse(yaml).unwrap())
    }

    const TWO: &str = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { os: debian@12, networks: { lan: 10 }, app: { image: redis, steps: [{ run: a.sh }] } }\n  b: { os: debian@12, networks: { lan: 11 }, app: { image: redis, steps: [{ run: b.sh }] } }\n";

    fn on(target: Target, variant: Option<&'static str>) -> Column {
        Column { target, variant }
    }

    #[test]
    fn vagrant_and_cloud_vm_have_a_column_per_provider_and_per_cloud() {
        let ids: Vec<String> = columns().iter().map(Column::id).collect();
        assert_eq!(ids.len(), Target::ALL.len() - 2 + 8 + 6);
        assert!(ids.contains(&"vagrant/libvirt".to_owned()) && ids.contains(&"cloud-vm/oci".to_owned()));
        assert!(!ids.contains(&"vagrant".to_owned()) && ids.contains(&"docker-vm".to_owned()));
    }

    /// What `column` refuses `lab` for; a test failure when it doesn't.
    fn refused_on(lab: &Lab, column: Column) -> Refused {
        match decide(lab, &column) {
            Decision::Refused(r) => r,
            other => panic!("{}: {other:?}", column.id()),
        }
    }

    #[test]
    fn each_step_refuses_with_its_own_code() {
        let two = lab(TWO);
        assert_eq!(decide(&two, &on(Target::Docker, None)), Decision::Generated);
        let cloud = refused_on(&two, on(Target::CloudVm, Some("digitalocean")));
        assert_eq!(cloud.code, Code::Cloud, "{cloud:?}");
        assert!(cloud.reason.contains("one machine on one network"), "{cloud:?}");
        let image = lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: redis } }\n");
        let lowering = refused_on(&image, on(Target::Vagrant, Some("virtualbox")));
        assert_eq!((lowering.code, lowering.field.as_deref()), (Code::Lowering, Some("machines.a.app.image")));
        let aliased = lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, aliases: [www.x.test], app: { image: redis } }\n",
        );
        let lacking = refused_on(&aliased, on(Target::Kubernetes, None));
        assert_eq!(lacking.code, Code::Capability(Feature::MachineAliases), "{lacking:?}");
        let provider = refused_on(&two, on(Target::Vagrant, Some("utm")));
        assert_eq!(provider.code, Code::Provider, "{provider:?}");
    }

    #[test]
    fn a_lab_a_target_doesnt_take_is_not_applicable_there() {
        let two = lab(TWO);
        assert_eq!(decide(&two, &on(Target::CloudServices, None)), Decision::NotApplicable);
        assert_eq!(decide(&two, &on(Target::Hybrid, None)), Decision::NotApplicable, "no VM-only machine");
    }
}
