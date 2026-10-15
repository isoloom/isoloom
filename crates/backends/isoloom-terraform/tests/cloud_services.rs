//! The cloud-services scripts with every `cloud:` key that shapes them (`before`, `env`,
//! `apply_retries`, inputs at any depth), as snapshots.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_services::CloudServicesBackend;

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

fn file(name: &str) -> String {
    let spec = isoloom_spec::parse(SPEC).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    CloudServicesBackend
        .render(&Environment::build(&spec, Target::CloudServices, None))
        .unwrap()
        .into_iter()
        .find(|f| f.path == format!(".isoloom/cloud-services/{name}"))
        .unwrap()
        .contents
}

#[test]
fn up_copies_the_module_runs_the_steps_and_retries() {
    insta::assert_snapshot!(file("up.sh"));
}

#[test]
fn down_destroys_with_the_copy_as_applied() {
    insta::assert_snapshot!(file("down.sh"));
}

#[test]
fn outputs_read_the_state() {
    insta::assert_snapshot!(file("outputs.sh"));
}

#[test]
fn inputs_stay_out_of_the_fixed_variables() {
    insta::assert_snapshot!(file("terraform.tfvars.json"));
}
