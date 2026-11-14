//! The `cloud-services` target's files: the module's fixed variables; and how the `cloud:` keys
//! are validated.

use isoloom_spec::Target;
use isoloom_targets::Lab;

use crate::testing::{Outcome, generated, lab, problems};

/// A cloud-services spec on AWS with `fields` added.
fn spec(fields: &str) -> Outcome<Lab> {
    lab(&format!("version: 1\nname: t\ncloud:\n  provider: aws\n  terraform: tf\n{fields}"))
}

/// The cloud-services file `name`.
fn script(spec: &Lab, name: &str) -> Outcome<String> {
    generated(spec, Target::CloudServices, &format!(".isoloom/cloud-services/{name}"))
}

#[test]
fn the_target_writes_the_fixed_variables_and_no_script() -> Outcome {
    let s = spec("")?;
    assert!(script(&s, "terraform.tfvars.json").is_ok());
    // No script: nothing sh-only is generated, and no run manifest.
    for gone in ["up.sh", "down.sh", "outputs.sh", "run.json"] {
        assert!(script(&s, gone).is_err(), "{gone}");
    }
    Ok(())
}

#[test]
fn env_retries_and_before_steps_are_valid_keys() -> Outcome {
    let s = spec("  env: { AWS_REGION: us-east-1, QUOTED: \"it's\" }\n  apply_retries: 2\n")?;
    assert!(problems(&s).is_empty(), "{:?}", problems(&s));
    let with_steps = spec("  before: [keys.sh, 'scripts/my build.sh']\n")?;
    assert!(script(&with_steps, "terraform.tfvars.json").is_ok());
    Ok(())
}

#[test]
fn a_variable_holding_an_input_anywhere_stays_out_of_the_fixed_ones() -> Outcome {
    let s = spec("  vars: { allow: [\"{{ inputs.IP }}/32\"], region: eu-west-3 }\ninputs: [IP]\n")?;
    assert_eq!(script(&s, "terraform.tfvars.json")?, "{\n  \"region\": \"eu-west-3\"\n}\n");
    let snapshot = isoloom_targets::resolved::resolve(&s);
    assert_eq!(snapshot["cloud"]["inputs"], serde_json::json!(["IP"]));
    Ok(())
}

#[test]
fn new_keys_are_validated() -> Outcome {
    let bad = spec(
        "  env: { 'AWS-REGION': x, 1X: y, here: z, PATH: /bin, TF_DATA_DIR: d, ISOLOOM_PROJECT: p, TF_VAR_isoloom_instance: i }\n  apply_retries: 9\n  requires: [' ']\n",
    )?;
    let found = problems(&bad);
    for want in [
        "cloud.env.AWS-REGION",
        "cloud.env.1X",
        "cloud.env.here: `here` is the scripts' own",
        "cloud.env.PATH: `PATH` is the scripts' own",
        "cloud.env.TF_DATA_DIR",
        "cloud.env.ISOLOOM_PROJECT",
        "cloud.env.TF_VAR_isoloom_instance",
        "cloud.apply_retries: at most 5",
        "cloud.requires[0]",
    ] {
        assert!(found.iter().any(|p| p.contains(want)), "{want} not in {found:?}");
    }
    Ok(())
}

#[test]
fn a_before_step_must_exist_in_the_project() -> Outcome {
    let dir = tempfile::Builder::new().prefix("isoloom-cloud-before-").tempdir()?;
    let dir = dir.path();
    std::fs::create_dir_all(dir.join("tf")).unwrap();
    std::fs::write(dir.join("tf/main.tf"), "").unwrap();
    let s = spec("  before: [keys.sh, ../out.sh]\n")?;
    let found: Vec<String> = s.problems_with_files(dir).iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert!(found.iter().any(|p| p.contains("cloud.before[0]") && p.contains("doesn't exist")), "{found:?}");
    assert!(
        found.iter().any(|p| p.contains("cloud.before[1]") && p.contains("inside the project")),
        "{found:?}"
    );
    Ok(())
}
