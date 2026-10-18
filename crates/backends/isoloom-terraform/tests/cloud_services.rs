//! The cloud-services target with every `cloud:` key that shapes it (`before`, `env`,
//! `apply_retries`, `requires`, inputs whole or inside a value): its variables file, and the run
//! manifest the runner executes, or none when the services need what a manifest can't hold.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::CloudServicesBackend;
use testing::{Outcome, spec};

/// Everything a manifest can't hold: `before` steps, an input inside a larger value.
const SPEC: &str = r#"version: 1
name: goat
inputs: [PLAYER_IP]
cloud:
  provider: aws
  terraform: terraform/
  vars: { cg_whitelist: ["{{ inputs.PLAYER_IP }}/32"], region: us-east-1 }
  outputs: { site: url }
  before: [keys.sh, "scripts/my build.sh"]
  env: { AWS_REGION: us-east-1, QUOTED: "it's" }
  apply_retries: 2
  requires: [jq, "python3 -c 'import boto3'"]
"#;

/// What a manifest holds: inputs as whole values, `env`, retries, `requires`.
const RUNNABLE: &str = r#"version: 1
name: goat
inputs: [PLAYER_CIDR, TOKEN]
cloud:
  provider: aws
  terraform: terraform/
  vars: { cg_whitelist: "{{ inputs.PLAYER_CIDR }}", token: "{{ inputs.TOKEN }}", api_token: "{{ inputs.TOKEN }}", region: us-east-1 }
  outputs: { site: url, admin-password: admin_password }
  env: { AWS_REGION: us-east-1 }
  apply_retries: 5
  requires: [jq, "aws --version"]
"#;

/// The variables file written for `yaml`.
fn vars_file(yaml: &str) -> Outcome<String> {
    let files = CloudServicesBackend.render(&testing::planned(&spec(yaml)?, Target::CloudServices))?;
    Ok(testing::file(&files, ".isoloom/cloud-services/terraform.tfvars.json")?.to_owned())
}

/// The run manifests written for `yaml`, as JSON.
fn manifests(yaml: &str) -> Outcome<Vec<String>> {
    let placed = CloudServicesBackend.run_manifests(&testing::planned(&spec(yaml)?, Target::CloudServices))?;
    Ok(placed.into_iter().map(|p| isoloom_manifest::to_json(&p.manifest)).collect::<Result<_, _>>()?)
}

#[test]
fn inputs_stay_out_of_the_fixed_variables() -> Outcome {
    insta::assert_snapshot!(vars_file(SPEC)?);
    Ok(())
}

#[test]
fn before_steps_or_an_input_inside_a_value_leave_the_lab_without_a_run_manifest() -> Outcome {
    assert_eq!(manifests(SPEC)?.len(), 0);
    let only_before = SPEC.replace("[\"{{ inputs.PLAYER_IP }}/32\"]", "\"{{ inputs.PLAYER_IP }}\"");
    assert_eq!(manifests(&only_before)?.len(), 0, "`before` alone");
    assert_eq!(manifests(&only_before.replace("  before: [keys.sh, \"scripts/my build.sh\"]\n", ""))?.len(), 1);
    Ok(())
}

#[test]
fn the_runner_copies_the_module_applies_it_with_the_launch_values_and_reads_the_outputs() -> Outcome {
    let all = manifests(RUNNABLE)?;
    let [json] = all.as_slice() else { return Err("one manifest".into()) };
    let m = isoloom_manifest::parse(json)?;
    m.validate()?;
    insta::assert_snapshot!(json);
    Ok(())
}
