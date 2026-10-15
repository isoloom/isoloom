//! The `cloud-services` target: an environment of cloud services (`cloud:`), its Terraform
//! module applied into the user's own cloud account ([`CloudServicesBackend`]).
//!
//! The module is the project's own and stays untouched: Isoloom writes, under
//! `.isoloom/cloud-services/`, its variables (`terraform.tfvars.json`) and three scripts that run
//! Terraform on a copy of it (`module/`) with the state and the provider plugins kept here, not
//! in the project: `up.sh` (the copy, the `before` steps, init and apply), `down.sh` (destroy)
//! and `outputs.sh` (the outputs, as JSON). The cloud's credentials come from the environment,
//! as Terraform expects.

mod scripts;

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::model::{CloudServices, Spec, Target};
use isoloom_spec::shell;

/// From `.isoloom/cloud-services/` back to the project folder.
const ROOT: &str = "../..";
/// Terraform's plugins and modules, next to the scripts.
const INIT: &str = "terraform -chdir=\"$module\" init -input=false >&2\n";
/// The fixed variables, then the launch-time inputs `run` wrote (when the spec takes any).
const VAR_FILES: &str =
    "set -- -var-file=\"$here/terraform.tfvars.json\"\n[ ! -f \"$here/inputs.tfvars.json\" ] || set -- \"$@\" -var-file=\"$here/inputs.tfvars.json\"\n";
/// The state, next to the scripts.
const STATE: &str = "-state=\"$here/terraform.tfstate\"";

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
        let cloud = spec.cloud.as_ref().ok_or(GenerateError::NotPossible(Target::CloudServices))?;
        Ok(files(spec, cloud))
    }
}

/// The variables and the three scripts.
fn files(spec: &Spec, cloud: &CloudServices) -> Files {
    let path = |f: &str| format!("{OUTPUT_DIR}/{}/{f}", Target::CloudServices.id());
    let file = |f: &str, contents: String| GeneratedFile { path: path(f), contents };
    let script = |what: &str, body: String| format!("{}{body}", prelude(spec, cloud, what));
    let apply = scripts::with_retries(
        &format!("terraform -chdir=\"$module\" apply -auto-approve -input=false {STATE} \"$@\""),
        cloud.apply_retries,
    );
    vec![
        file("terraform.tfvars.json", vars_json(cloud)),
        file(
            "up.sh",
            script(
                "Creates the services (`isoloom run cloud-services`).",
                format!("{}{}{INIT}{VAR_FILES}{apply}", scripts::MARKER, scripts::copy(cloud)),
            ),
        ),
        file(
            "down.sh",
            script(
                "Destroys them (`isoloom down cloud-services`): what they cost stops here.",
                format!(
                    "{}{INIT}{VAR_FILES}terraform -chdir=\"$module\" destroy -auto-approve -input=false {STATE} \"$@\"\n",
                    scripts::copy_if_gone(cloud)
                ),
            ),
        ),
        file(
            "outputs.sh",
            script(
                "Prints the module's outputs as JSON (what `isoloom test` and the message read).",
                format!("{}terraform -chdir=\"$module\" output -json {STATE}\n", scripts::OUTPUTS_MODULE),
            ),
        ),
    ]
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

/// Every script starts in its own folder, keeps Terraform's plugins and state there, and runs
/// Terraform on its copy of the module (`-chdir`), whatever the caller's working directory.
fn prelude(spec: &Spec, cloud: &CloudServices, what: &str) -> String {
    let src = shell::quote(&format!("{ROOT}/{}", cloud.terraform.trim_end_matches('/')));
    format!(
        "#!/bin/sh\n{}# {what}\n# The services of `{}`: the Terraform module `{}`, in the user's {} account.\nset -eu\ncd \"$(dirname \"$0\")\"\nhere=\"$(pwd)\"\nexport TF_DATA_DIR=\"$here/.terraform\"\n{}",
        header("#"),
        spec.name,
        cloud.terraform,
        cloud.provider.id(),
        scripts::environment(cloud, &src)
    )
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_render::backend::Backend;
    use isoloom_spec::model::Target;

    use super::CloudServicesBackend;

    fn env(yaml: &str) -> Environment {
        let lab = isoloom_plan::Lab::new(isoloom_spec::parse(yaml).unwrap());
        Environment::build(&lab.for_target(Target::CloudServices), Target::CloudServices, None)
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
        assert_eq!(files[0].contents, "{\n  \"region\": \"eu-west-3\"\n}\n");
        assert!(files[1].contents.contains("src='../../tf'\n"), "{}", files[1].contents);
        assert!(CloudServicesBackend.refusal(&e).is_none());
    }
}
