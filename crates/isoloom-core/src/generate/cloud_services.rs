//! The `cloud-services` target: an environment of cloud services (`cloud:`), its Terraform
//! module applied into the user's own cloud account.
//!
//! The module is the project's own and stays untouched: Isoloom writes, under
//! `.isoloom/cloud-services/`, its variables (`terraform.tfvars.json`) and three scripts that run
//! Terraform on it with the state and the provider plugins kept here, not in the module:
//! `up.sh` (init and apply), `down.sh` (destroy) and `outputs.sh` (the outputs, as JSON). The
//! cloud's credentials come from the environment, as Terraform expects.

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::model::Spec;

const DIR: &str = "cloud-services";
/// From `.isoloom/cloud-services/` back to the project folder.
const ROOT: &str = "../..";

/// Single-quoted for sh.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    let cloud = spec.cloud.as_ref().expect("the cloud-services target needs cloud:");
    let module = sq(&format!("{ROOT}/{}", cloud.terraform.trim_end_matches('/')));
    let vars: serde_json::Map<String, serde_json::Value> = cloud.vars.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
    let mut vars_json = serde_json::to_string_pretty(&serde_json::Value::Object(vars)).expect("vars serialize");
    vars_json.push('\n');

    // Every script starts in its own folder, keeps Terraform's plugins and state there, and runs
    // Terraform on the module (`-chdir`), whatever the caller's working directory.
    let prelude = |what: &str| {
        let mut s = String::from("#!/bin/sh\n");
        s.push_str(&header("#"));
        s.push_str(&format!("# {what}\n"));
        s.push_str(&format!(
            "# The services of `{}`: the Terraform module `{}`, in the user's {} account.\nset -eu\ncd \"$(dirname \"$0\")\"\nhere=\"$(pwd)\"\nexport TF_DATA_DIR=\"$here/.terraform\"\nmodule={module}\n",
            spec.name,
            cloud.terraform,
            cloud.provider.id()
        ));
        s
    };
    let init = "terraform -chdir=\"$module\" init -input=false >&2\n";
    let state = "-state=\"$here/terraform.tfstate\"";
    let up = format!(
        "{}{init}terraform -chdir=\"$module\" apply -auto-approve -input=false {state} -var-file=\"$here/terraform.tfvars.json\"\n",
        prelude("Creates the services (`isoloom run cloud-services`).")
    );
    let down = format!(
        "{}{init}terraform -chdir=\"$module\" destroy -auto-approve -input=false {state} -var-file=\"$here/terraform.tfvars.json\"\n",
        prelude("Destroys them (`isoloom down cloud-services`): what they cost stops here.")
    );
    let outputs = format!(
        "{}terraform -chdir=\"$module\" output -json {state}\n",
        prelude("Prints the module's outputs as JSON (what `isoloom test` and the message read).")
    );
    let path = |f: &str| format!("{OUTPUT_DIR}/{DIR}/{f}");
    Ok(vec![
        GeneratedFile {
            path: path("terraform.tfvars.json"),
            contents: vars_json,
        },
        GeneratedFile {
            path: path("up.sh"),
            contents: up,
        },
        GeneratedFile {
            path: path("down.sh"),
            contents: down,
        },
        GeneratedFile {
            path: path("outputs.sh"),
            contents: outputs,
        },
    ])
}
