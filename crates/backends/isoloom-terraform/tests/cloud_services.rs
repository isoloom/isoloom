//! The cloud-services target with every `cloud:` key that shapes it (`before`, `env`,
//! `apply_retries`, `requires`, inputs whole or inside a value): its variables file.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::CloudServicesBackend;
use testing::{Outcome, spec};

/// Every key: `before` steps, an input inside a larger value.
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

/// The variables file written for `yaml`.
fn vars_file(yaml: &str) -> Outcome<String> {
    let files = CloudServicesBackend.render(&testing::planned(&spec(yaml)?, Target::CloudServices)?)?;
    Ok(testing::file(&files, ".isoloom/cloud-services/terraform.tfvars.json")?.to_owned())
}

#[test]
fn inputs_stay_out_of_the_fixed_variables() -> Outcome {
    insta::assert_snapshot!(vars_file(SPEC)?);
    Ok(())
}
