//! The `cloud-services` target: an environment of cloud services (`cloud:`), its Terraform
//! module applied into the user's own cloud account ([`CloudServicesBackend`]).
//!
//! The module is the project's own and stays untouched: Isoloom writes, under
//! `.isoloom/cloud-services/`, its fixed variables (`terraform.tfvars.json`) and the run manifest
//! the Isothrum runner executes ([`manifest`]): the module copied into the run's own folder,
//! applied there with the state and the plugins kept with the run, never in the project. The
//! cloud's credentials come from the environment, as Terraform expects.

mod manifest;

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{self, Backend, Files, Placed};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::{CloudServices, Target};

/// The `cloud-services` backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CloudServicesBackend;

impl Backend for CloudServicesBackend {
    fn target(&self) -> Target {
        Target::CloudServices
    }

    /// None: a spec of cloud services has no machines, so it asks for nothing a machine does
    /// (provisioning, playbook checks...); whatever it asks for is refused.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of([])
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudServices, env, &self.capabilities())?;
        let spec = env.spec();
        let cloud = spec.cloud.as_ref().ok_or_else(|| RenderError::Unsupported {
            target: Target::CloudServices,
            what: "not possible for this spec (see `isoloom targets`)".to_owned(),
        })?;
        Ok(vec![vars_file(cloud)])
    }

    /// The run manifest, beside the variables, unless the services need what a manifest can't
    /// hold (then the lab is converted, not run).
    fn run_manifests(&self, env: &Environment) -> Result<Vec<Placed>, RenderError> {
        let Some(cloud) = env.spec().cloud.as_ref().filter(|c| manifest::refusal(c).is_none()) else {
            return Ok(Vec::new());
        };
        let m = manifest::manifest(env, cloud).map_err(|source| crate::Error::Manifest {
            target: Target::CloudServices,
            source,
        })?;
        Ok(vec![Placed::here(m)])
    }
}

/// The fixed variables, `.isoloom/cloud-services/terraform.tfvars.json`.
fn vars_file(cloud: &CloudServices) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/terraform.tfvars.json", Target::CloudServices.id()),
        contents: vars_json(cloud),
    }
}

/// The fixed variables only: a variable taking a launch-time input, whole or inside a string,
/// is written by `run`, from the environment, to `inputs.tfvars.json` next to the state (kept
/// for `down`).
fn vars_json(cloud: &CloudServices) -> String {
    let vars: serde_json::Map<String, serde_json::Value> = cloud
        .vars
        .iter()
        .filter(|(_, v)| CloudServices::inputs_in(v).is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // A map of JSON values always serializes; the pretty form is what a reader expects.
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Object(vars)).unwrap_or_else(|_| "{}".to_owned());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_render::backend::Backend;
    use isoloom_spec::model::Target;

    use super::CloudServicesBackend;

    fn env(yaml: &str) -> Environment {
        crate::testing::planned(yaml, Target::CloudServices)
    }

    #[test]
    fn a_spec_without_cloud_is_not_possible() {
        let e = env("version: 1\nname: x\nnetworks: {}\nmachines: {}\n");
        let err = CloudServicesBackend.render(&e).unwrap_err();
        assert!(err.to_string().contains("not possible"), "{err}");
    }

    #[test]
    fn inputs_stay_out_of_the_fixed_variables() {
        let e = env(
            "version: 1\nname: x\ninputs: [TOKEN]\ncloud:\n  provider: aws\n  terraform: tf/\n  vars: { region: eu-west-3, token: \"{{ inputs.TOKEN }}\" }\n",
        );
        let files = CloudServicesBackend.render(&e).unwrap();
        assert_eq!(files.len(), 1, "the variables only: the runner runs the module");
        assert_eq!(files[0].contents, "{\n  \"region\": \"eu-west-3\"\n}\n");
        assert!(CloudServicesBackend.refusal(&e).is_none());
    }
}
