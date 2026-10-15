//! The cloud-services scripts with every `cloud:` key that shapes them (`before`, `env`,
//! `apply_retries`, inputs at any depth), as snapshots.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::CloudServicesBackend;
use testing::{Outcome, spec};

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

/// The script or variables file `name` written for `SPEC`.
fn file(name: &str) -> Outcome<String> {
    let files = CloudServicesBackend.render(&testing::planned(&spec(SPEC)?, Target::CloudServices))?;
    Ok(testing::file(&files, &format!(".isoloom/cloud-services/{name}"))?.to_owned())
}

#[test]
fn up_copies_the_module_runs_the_steps_and_retries() -> Outcome {
    insta::assert_snapshot!(file("up.sh")?);
    Ok(())
}

#[test]
fn down_destroys_with_the_copy_as_applied() -> Outcome {
    insta::assert_snapshot!(file("down.sh")?);
    Ok(())
}

#[test]
fn outputs_read_the_state() -> Outcome {
    insta::assert_snapshot!(file("outputs.sh")?);
    Ok(())
}

#[test]
fn inputs_stay_out_of_the_fixed_variables() -> Outcome {
    insta::assert_snapshot!(file("terraform.tfvars.json")?);
    Ok(())
}
