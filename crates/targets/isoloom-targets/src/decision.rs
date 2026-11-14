//! What a target decides about an environment, typed: it generates it, or it refuses it and at which step
//! (lowering, the backend's capabilities, the backend's own limits, a Vagrant provider's, a
//! cloud's), or the environment isn't one the target takes at all. The one place a target decides:
//! [`refusal`], [`refusal_on`] and the capability matrix ([`crate::matrix`]) read these
//! decisions, so the matrix is what the generators decide and nothing more.
//!
//! [`refusal`]: crate::refusal()
//! [`refusal_on`]: crate::refusal_on

use isoloom_plan::{Environment, Feature, Lab};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};

use crate::GenerateError;
use crate::generation::planned;
use crate::registry::backend;
use crate::tiers::VAGRANT_PROVIDERS;

/// A target, or one of its Vagrant providers or clouds where each decides on its own: a column
/// of the capability matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Column {
    /// The target.
    pub target: Target,
    /// The Vagrant provider (`libvirt`) or the cloud (`aws`), for `vagrant` and `cloud-vm`.
    pub variant: Option<&'static str>,
}

impl Column {
    /// Its name: the target's id, then `/` and the provider or cloud (`vagrant/libvirt`).
    #[must_use]
    pub(crate) fn id(&self) -> String {
        crate::tiers::name(self.target, self.variant)
    }
}

/// Every column, in the order targets are listed: `vagrant` once per provider, `cloud-vm` once
/// per cloud, every other target once.
#[must_use]
pub(crate) fn columns() -> Vec<Column> {
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

/// The step that refuses an environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub(crate) enum Code {
    /// The spec isn't valid, or not once lowered for the target.
    Invalid,
    /// Lowering: a machine has no form the target runs, or a step the target doesn't run,
    /// at the field that rules it out.
    Lowering,
    /// The target's backend lacks the capability the environment needs.
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
    pub(crate) fn id(self) -> &'static str {
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
    pub(crate) fn feature(self) -> Option<Feature> {
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
pub(crate) struct Refused {
    /// The step that refuses.
    pub code: Code,
    /// The field, as written in the spec (`machines.web.app.image`), when the step names one.
    pub field: Option<String>,
    /// Why, as the generator says it.
    pub reason: String,
}

impl Refused {
    /// The refusal on one line, as `isoloom targets` and the resolved snapshot give it: the field
    /// (when the step names one) then the reason, an invalid spec's said so.
    #[must_use]
    pub(crate) fn line(&self) -> String {
        match (&self.field, self.code) {
            (Some(field), Code::Invalid) => format!("the spec is invalid: {field}: {}", self.reason),
            (Some(field), _) => format!("{field}: {}", self.reason),
            (None, _) => self.reason.clone(),
        }
    }
}

/// What a column decides about an environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Decision {
    /// It generates the environment.
    Generated,
    /// It refuses it.
    Refused(Refused),
    /// The environment isn't one the target takes: cloud services on a machine target, machines on
    /// `cloud-services`, a target `targets:` leaves out.
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

/// What `target` decides about `lab`, run with `variant` when it names one: the spec's own
/// problems, lowering, then the backend's capabilities and its own limits (a cloud's, on
/// `cloud-vm` with a cloud: `aws`), then a provider's: a Vagrant provider's on `vagrant`
/// (`libvirt`).
///
/// # Errors
///
/// [`GenerateError::Internal`] when the environment doesn't plan for `target` (a bug of
/// lowering, or a catalogue that doesn't load), as the generation reports it: never a refusal
/// that would leave the target out, nor a target said to be generated.
pub(crate) fn decide(lab: &Lab, target: Target, variant: Option<&str>) -> Result<Decision, GenerateError> {
    if let Some(p) = lab.problems().into_iter().next() {
        return Ok(refused(Code::Invalid, Some(p.path.to_string()), p.reason));
    }
    if let Some(p) = lab.refusal(target) {
        return Ok(refused(Code::Lowering, Some(p.path.to_string()), p.reason));
    }
    if !lab.possible().contains(&target) {
        return Ok(Decision::NotApplicable);
    }
    let spec = lab.for_target(target);
    if let Some(p) = isoloom_spec::validate::validate(&spec).first() {
        return Ok(refused(Code::Invalid, None, format!("the spec is invalid on {}: {p}", target.id())));
    }
    let backend = backend(target);
    let env = planned(lab, &spec, target, None)?;
    let lacking = isoloom_plan::refusal(&env.requirements, &backend.capabilities());
    let own = match (target, variant.and_then(cloud)) {
        (Target::CloudVm, Some(c)) => CloudVmModule::new(c).refusal(&env).map(|r| (Code::Cloud, r)),
        _ => backend.refusal(&env).map(|r| (Code::Target, r)),
    };
    Ok(match (lacking, own) {
        (Some(lack), Some((_, reason))) => refused(Code::Capability(lack.requirement.feature()), None, reason),
        (Some(lack), None) => refused(Code::Capability(lack.requirement.feature()), None, lack.to_string()),
        (None, Some((code, reason))) => refused(code, None, reason),
        (None, None) => provider_refusal(&env, target, variant).map_or(Decision::Generated, |r| refused(Code::Provider, None, r)),
    })
}

/// Why `target`'s `provider` can't run the environment it otherwise generates, if it can't.
fn provider_refusal(env: &Environment, target: Target, provider: Option<&str>) -> Option<String> {
    match (target, provider?) {
        (Target::Vagrant, p) => isoloom_vagrant::provider_refusal(env, p),
        _ => None,
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
        assert_eq!(ids.len(), Target::ALL.len() - 2 + 7 + 6);
        assert!(ids.contains(&"vagrant/libvirt".to_owned()) && ids.contains(&"cloud-vm/oci".to_owned()));
        assert!(!ids.contains(&"vagrant".to_owned()) && ids.contains(&"cloud-docker".to_owned()));
    }

    /// What `column` refuses `lab` for; a test failure when it doesn't.
    fn refused_on(lab: &Lab, column: Column) -> Refused {
        match decide(lab, column.target, column.variant).unwrap() {
            Decision::Refused(r) => r,
            other => panic!("{}: {other:?}", column.id()),
        }
    }

    #[test]
    fn a_lab_the_target_takes_is_generated() {
        assert_eq!(decide(&lab(TWO), Target::Docker, None).unwrap(), Decision::Generated);
    }

    #[test]
    fn the_cloud_step_refuses_with_the_cloud_code() {
        let cloud = refused_on(&lab(TWO), on(Target::CloudVm, Some("digitalocean")));
        assert_eq!(cloud.code, Code::Cloud, "{cloud:?}");
        assert!(cloud.reason.contains("one machine on one network"), "{cloud:?}");
    }

    #[test]
    fn the_lowering_step_refuses_with_the_lowering_code_and_the_field() {
        let image = lab("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: redis } }\n");
        let lowering = refused_on(&image, on(Target::Vagrant, Some("virtualbox")));
        assert_eq!((lowering.code, lowering.field.as_deref()), (Code::Lowering, Some("machines.a.app.image")));
    }

    #[test]
    fn the_capability_step_refuses_with_the_missing_feature() {
        let aliased = lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, aliases: [www.x.test], app: { image: redis } }\n",
        );
        let lacking = refused_on(&aliased, on(Target::Kubernetes, None));
        assert_eq!(lacking.code, Code::Capability(Feature::MachineAliases), "{lacking:?}");
    }

    #[test]
    fn tools_are_refused_where_no_backend_renders_them() {
        // `tools:` was generated without its toolbox on these targets (their files were the same
        // with or without it): each now refuses it, and the targets that render it still do.
        let tools = "tools: { shell: {} }\n";
        let containers = lab(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, app: {{ image: redis }} }}\n{tools}"
        ));
        let vms = lab(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ os: debian@12, networks: {{ lan: 10 }}, app: {{ steps: [{{ run: a.sh }}] }} }}\n{tools}"
        ));
        assert_eq!(decide(&containers, Target::Docker, None), Decision::Generated);
        assert_eq!(decide(&vms, Target::Vagrant, Some("virtualbox")), Decision::Generated);
        for (lab, column) in [
            (&containers, on(Target::Kubernetes, None)),
            (&vms, on(Target::Proxmox, None)),
            (&vms, on(Target::Esxi, None)),
            (&vms, on(Target::CloudVm, Some("aws"))),
        ] {
            let lacking = refused_on(lab, column);
            assert_eq!(lacking.code, Code::Capability(Feature::Tools), "{lacking:?}");
            assert!(lacking.reason.contains("tool `shell` (`tools:`)"), "{lacking:?}");
        }
    }

    #[test]
    fn the_provider_step_refuses_with_the_provider_code() {
        let two_networks = lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 }, wan: { cidr: 10.9.1.0/24 } }\nmachines:\n  a: { os: debian@12, networks: { lan: 10, wan: 10 }, app: { steps: [{ run: a.sh }] } }\n",
        );
        let provider = refused_on(&two_networks, on(Target::Vagrant, Some("qemu")));
        assert_eq!(provider.code, Code::Provider, "{provider:?}");
    }

    #[test]
    fn a_lab_a_target_doesnt_take_is_not_applicable_there() {
        let two = lab(TWO);
        assert_eq!(decide(&two, Target::CloudServices, None).unwrap(), Decision::NotApplicable);
    }
}
