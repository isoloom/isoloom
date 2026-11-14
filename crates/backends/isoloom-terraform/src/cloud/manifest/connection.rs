//! What a cloud run is given at run time beyond the environment's own values: the module's
//! variables as params (the region, the account, the sizes, who may reach the machines), the
//! provider's profile, and the provider's credentials as connection slots.
//!
//! The credentials reach Terraform's own steps only, as the provider's environment: never a
//! command line, never a file, never the SSH sessions to the machines. Each slot also names the
//! runner's environment as its source, so a command line run with the cloud's variables
//! exported just works; a program embedding a runner can give them another way. The profile,
//! region and account are params, so the runner records them at `up` and a `down` replays them:
//! the runner gives Terraform no ambient cloud variable of its own.

use std::collections::BTreeSet;

use isoloom_manifest::{
    Delivery, EnvName, Error as ManifestError, Identifier, NonEmpty, Param, SecretSlot, SecretSource, SetBy, SlotKind, StepId, StepSelection, Store,
};

use crate::cloud::Cloud;
use crate::module::variable::Variable;

/// The module's variables the runner sets itself: never params the embedder gives.
pub(crate) const RUNNER_SET: [&str; 3] = ["ssh_public_key", "ssh_private_key_file", "expires_at"];

/// The Terraform steps, which alone get the provider's credentials and profile.
pub(crate) const TERRAFORM_STEPS: &[&str] = &["init", "apply", "destroy"];

/// The params: each of the module's variables an embedder gives (required when it has no
/// default, or when it is the account a destroy must find again), and the cloud's profile.
pub(crate) fn params(variables: &[Variable], cloud: Cloud) -> Result<Vec<Param>, ManifestError> {
    let mut out: Vec<Param> = variables
        .iter()
        .filter(|v| !v.is_sensitive() && !RUNNER_SET.contains(&v.name()))
        .map(|v| variable(v, account_of(cloud) == Some(v.name())))
        .collect::<Result<_, _>>()?;
    out.extend(profile(cloud, TERRAFORM_STEPS)?);
    Ok(out)
}

/// The variable naming the account a cloud's resources are in, which a destroy must find again
/// (required: with no ambient variable the provider would otherwise refuse, never guess).
fn account_of(cloud: Cloud) -> Option<&'static str> {
    match cloud {
        Cloud::Azure => Some("subscription_id"),
        _ => None,
    }
}

/// Variable `v` as a param the embedder gives: required when it has no default (or `required`),
/// a `bool` taking `true` or `false`.
fn variable(v: &Variable, required: bool) -> Result<Param, ManifestError> {
    crate::module::manifest::param(v, required)
}

/// The Terraform variable `name`.
fn tf_var(name: &str) -> Result<NonEmpty<Delivery>, ManifestError> {
    crate::module::manifest::tf_var_string(name)
}

/// The provider's environment variable `var`, on the Terraform `steps` only.
pub(crate) fn on_terraform(var: &str, steps: &[&str]) -> Result<NonEmpty<Delivery>, ManifestError> {
    let steps: Vec<StepId> = steps.iter().map(|s| StepId::parse(s)).collect::<Result<_, _>>()?;
    Ok(NonEmpty::one(Delivery::Env {
        var: EnvName::parse(var)?,
        steps: StepSelection::Only(NonEmpty::new(steps, "the Terraform steps")?),
    }))
}

/// The cloud's profile, when its provider reads one from the environment (AWS): a param on the
/// Terraform `steps`, so a destroy uses the profile the run was made with.
pub(crate) fn profile(cloud: Cloud, steps: &[&str]) -> Result<Vec<Param>, ManifestError> {
    let Cloud::Aws = cloud else { return Ok(Vec::new()) };
    Ok(vec![embedder_param(
        "AWS_PROFILE",
        "The AWS CLI profile the provider signs in with (its credentials and settings), when no keys are given.",
        on_terraform("AWS_PROFILE", steps)?,
    )?])
}

/// A param the embedder gives (no default, not required): its name, description and delivery.
fn embedder_param(name: &str, description: &str, deliver: NonEmpty<Delivery>) -> Result<Param, ManifestError> {
    Ok(Param {
        name: EnvName::parse(name)?,
        description: description.into(),
        default: None,
        values: Vec::new(),
        required: false,
        hosts: BTreeSet::new(),
        providers: BTreeSet::new(),
        set_by: SetBy::Embedder,
        deliver,
    })
}

/// Where a module of the environment's own (cloud-services) runs on `cloud`, as its provider reads it from
/// the environment (the region, the account, the project), on the Terraform `steps`; and the
/// module's `region` variable, which a module that declares one takes. Each a param, so the
/// runner records it at `up` and a destroy goes where the apply went. A name `taken` already
/// gives (the spec's own `env`) is left out.
pub(crate) fn placement(cloud: Cloud, steps: &[&str], taken: &dyn Fn(&str) -> bool) -> Result<Vec<Param>, ManifestError> {
    let env_names: &[(&str, &str)] = match cloud {
        Cloud::Aws => &[("AWS_REGION", "The AWS region the provider works in, when the module's provider names none.")],
        Cloud::Azure => &[("ARM_SUBSCRIPTION_ID", "The Azure subscription the provider works in.")],
        Cloud::Gcp => &[
            ("GOOGLE_PROJECT", "The Google Cloud project the provider works in."),
            ("GOOGLE_REGION", "The Google Cloud region the provider works in."),
        ],
        Cloud::DigitalOcean | Cloud::Linode | Cloud::Oci => &[],
    };
    let mut out = vec![embedder_param(
        "region",
        "The module's `region` variable, when it declares one and the spec's `vars` don't set it.",
        tf_var("region")?,
    )?];
    for (name, what) in env_names.iter().filter(|(n, _)| !taken(n)) {
        out.push(embedder_param(name, what, on_terraform(name, steps)?)?);
    }
    Ok(out)
}

/// The provider's credentials on `cloud`: each a connection slot on the Terraform `steps`, the
/// runner's own environment its source. Required only where the provider has no other way in
/// (a token); the others fall back to the cloud CLI's sign-in (a profile, `az login`, ADC,
/// `~/.oci/config`).
pub(crate) fn slots(cloud: Cloud, steps: &[&str]) -> Result<Vec<SecretSlot>, ManifestError> {
    let (names, required): (&[&str], bool) = match cloud {
        Cloud::Aws => (&["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN"], false),
        Cloud::Azure => (&["ARM_CLIENT_ID", "ARM_CLIENT_SECRET", "ARM_TENANT_ID"], false),
        Cloud::Gcp => (&["GOOGLE_OAUTH_ACCESS_TOKEN", "GOOGLE_CREDENTIALS"], false),
        Cloud::DigitalOcean => (&["DIGITALOCEAN_TOKEN"], true),
        Cloud::Linode => (&["LINODE_TOKEN"], true),
        Cloud::Oci => (&[], false),
    };
    names
        .iter()
        .map(|n| {
            Ok(SecretSlot {
                name: Identifier::parse(n)?,
                kind: SlotKind::Connection,
                required,
                sensitive: true,
                machines: Vec::new(),
                providers: BTreeSet::new(),
                source: Some(SecretSource {
                    store: Store::Env,
                    reference: (*n).to_owned(),
                }),
                default: None,
                deliver: on_terraform(n, steps)?,
            })
        })
        .collect()
}

/// The launch's value `name`, an input, for the machines that read it (`readers`): the module's
/// variable of the same name (`TF_VAR_<name>` on the Terraform steps), read in their user data;
/// empty when not given. `None` when no machine reads it.
pub(crate) fn input(name: &str, sensitive: bool, readers: &[isoloom_manifest::Name]) -> Result<Option<SecretSlot>, ManifestError> {
    if readers.is_empty() {
        return Ok(None);
    }
    Ok(Some(SecretSlot {
        name: Identifier::parse(name)?,
        kind: SlotKind::Input,
        required: false,
        sensitive,
        machines: readers.to_vec(),
        providers: BTreeSet::new(),
        source: None,
        default: Some(String::new()),
        deliver: on_terraform(&format!("TF_VAR_{name}"), TERRAFORM_STEPS)?,
    }))
}

/// A machine secret, read from its `source` (as on every target): the module's variable
/// `ISOLOOM_SECRET_<NAME>` (`TF_VAR_` on the Terraform steps), which the machines' user data
/// writes into `/var/lib/isoloom/secrets/<name>`.
pub(crate) fn machine_secret(name: &str, source: SecretSource, machines: &[isoloom_manifest::Name]) -> Result<SecretSlot, ManifestError> {
    let var = isoloom_manifest::contract::secret_variable(name);
    Ok(SecretSlot {
        name: Identifier::parse(name)?,
        kind: SlotKind::Secret,
        required: true,
        sensitive: true,
        machines: machines.to_vec(),
        providers: BTreeSet::new(),
        source: Some(source),
        default: None,
        deliver: on_terraform(&format!("TF_VAR_{var}"), TERRAFORM_STEPS)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_reach_the_terraform_steps_only_from_the_runners_environment() {
        let aws = slots(Cloud::Aws, TERRAFORM_STEPS).unwrap();
        let names: Vec<&str> = aws.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN"]);
        for s in &aws {
            assert!(s.sensitive && !s.required && s.kind == SlotKind::Connection);
            assert_eq!(s.source.as_ref().map(|x| x.store), Some(Store::Env));
            let Delivery::Env {
                steps: StepSelection::Only(steps),
                ..
            } = s.deliver.iter().next().unwrap()
            else {
                panic!()
            };
            let ids: Vec<&str> = steps.iter().map(StepId::as_str).collect();
            assert_eq!(ids, ["init", "apply", "destroy"]);
        }
        assert!(slots(Cloud::DigitalOcean, TERRAFORM_STEPS).unwrap()[0].required, "a token is the only way in");
        assert!(
            slots(Cloud::Oci, TERRAFORM_STEPS).unwrap().is_empty(),
            "the OCI provider reads its CLI configuration"
        );
    }

    #[test]
    fn the_runner_sets_the_key_and_the_expiry_and_the_account_is_required() {
        let vars = [
            Variable::new("region", "string").default(crate::hcl_builder::string("x")),
            Variable::new("subscription_id", "string").default(crate::hcl_builder::null()),
            Variable::new("allowed_cidr", "string"),
            Variable::new("ssh_public_key", "string"),
            Variable::new("ssh_private_key_file", "string"),
            Variable::new("expires_at", "string").default(crate::hcl_builder::string("")),
        ];
        let p = params(&vars, Cloud::Azure).unwrap();
        let named = |n: &str| p.iter().find(|x| x.name.as_str() == n);
        assert!(named("ssh_public_key").is_none() && named("ssh_private_key_file").is_none() && named("expires_at").is_none());
        assert!(named("allowed_cidr").is_some_and(|x| x.required && x.set_by == SetBy::Embedder));
        assert!(named("subscription_id").is_some_and(|x| x.required), "a destroy must find the same account");
        assert!(named("region").is_some_and(|x| !x.required && x.default.as_deref() == Some("x")));
        assert!(params(&vars[..1], Cloud::Aws).unwrap().iter().any(|x| x.name.as_str() == "AWS_PROFILE"));
    }
}
