//! The `cloud-services` target's scripts: Terraform on a copy of the module, the `before`
//! steps, `env`, `apply_retries`, the deployment's name; and how the new `cloud:` keys are
//! validated.

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

/// The cloud-services run manifest, parsed.
fn manifest(spec: &Lab) -> Outcome<isoloom_manifest::Manifest> {
    Ok(isoloom_manifest::parse(&script(spec, "run.json")?)?)
}

#[test]
fn the_runner_runs_terraform_on_a_copy_of_the_module() -> Outcome {
    let s = spec("")?;
    let m = manifest(&s)?;
    let json = script(&s, "run.json")?;
    assert!(json.contains("\"action\": \"terraform.copy_module\""), "{json}");
    assert!(json.contains("\"from\": \"../../tf\""), "{json}");
    assert!(json.contains("\"-chdir=${run.state_dir}/module\""), "{json}");
    assert!(json.contains("\"-state=${run.state_dir}/terraform.tfstate\""), "{json}");
    // Destroy copies the module only when its copy is gone (what the module wrote there stays).
    assert!(json.contains("\"when\": \"missing\""), "{json}");
    // The deployment's name, for the module's tags; no retry by default.
    assert!(json.contains("\"TF_VAR_isoloom_instance\": \"${run.name}\""), "{json}");
    assert_eq!(m.recovery.and_then(|r| r.apply_retries).map(u8::from), Some(0));
    // No script: nothing sh-only is generated.
    for gone in ["up.sh", "down.sh", "outputs.sh"] {
        assert!(script(&s, gone).is_err(), "{gone}");
    }
    Ok(())
}

#[test]
fn env_and_retries_reach_the_terraform_steps_and_before_steps_leave_no_manifest() -> Outcome {
    let s = spec("  env: { AWS_REGION: us-east-1, QUOTED: \"it's\" }\n  apply_retries: 2\n")?;
    let m = manifest(&s)?;
    let apply = m.lifecycle.up.iter().find(|x| x.id.as_str() == "apply").ok_or("apply")?;
    let env: Vec<(String, String)> = apply.env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert!(env.contains(&("AWS_REGION".into(), "us-east-1".into())), "{env:?}");
    assert!(env.contains(&("QUOTED".into(), "it's".into())), "{env:?}");
    assert_eq!(m.recovery.and_then(|r| r.apply_retries).map(u8::from), Some(2));
    // Host scripts can't be held by a manifest: the lab is converted, not run.
    let with_steps = spec("  before: [keys.sh, 'scripts/my build.sh']\n")?;
    assert!(script(&with_steps, "run.json").is_err());
    assert!(script(&with_steps, "terraform.tfvars.json").is_ok());
    assert!(problems(&s).is_empty(), "{:?}", problems(&s));
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
    let dir = std::env::temp_dir().join(format!("isoloom-cloud-before-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(dir.join("tf")).unwrap();
    std::fs::write(dir.join("tf/main.tf"), "").unwrap();
    let s = spec("  before: [keys.sh, ../out.sh]\n")?;
    let found: Vec<String> = s.problems_with_files(&dir).iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert!(found.iter().any(|p| p.contains("cloud.before[0]") && p.contains("doesn't exist")), "{found:?}");
    assert!(
        found.iter().any(|p| p.contains("cloud.before[1]") && p.contains("inside the project")),
        "{found:?}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
