//! The `cloud-services` target's scripts: Terraform on a copy of the module, the `before`
//! steps, `env`, `apply_retries`, the deployment's name; and how the new `cloud:` keys are
//! validated. What `cloud.requires` asks of the host is tested in `isoloom_runtime::host`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_core::{Target, generate, parse, validate};

fn spec(fields: &str) -> isoloom_core::Spec {
    parse(&format!("version: 1\nname: t\ncloud:\n  provider: aws\n  terraform: tf\n{fields}")).expect("parses")
}

fn script(spec: &isoloom_core::Spec, name: &str) -> String {
    let files = generate(spec, Target::CloudServices).unwrap();
    let path = format!(".isoloom/cloud-services/{name}");
    files.into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}")).contents
}

#[test]
fn terraform_runs_on_a_copy_of_the_module() {
    let s = spec("");
    let up = script(&s, "up.sh");
    assert!(up.contains("src='../../tf'\nmodule=\"$here/module\"\n"), "{up}");
    assert!(up.contains("tar -C \"$src\" -cf - --exclude=.terraform"), "{up}");
    // The old copy's Terraform files go before the new ones land.
    let (clean, tar) = (up.find("find \"$module\" -type f").unwrap(), up.find("tar -C").unwrap());
    assert!(clean < tar, "{up}");
    assert!(
        up.contains("-name '*.tf' -o -name '*.tf.json' -o -name '*.tfvars' -o -name '*.tfvars.json'"),
        "{up}"
    );
    assert!(up.contains("terraform -chdir=\"$module\" init"), "{up}");
    assert!(up.contains("terraform -chdir=\"$module\" apply -auto-approve"), "{up}");
    assert!(!up.contains("tries="), "no retries by default: {up}");
    // The deployment's name, for the module's tags.
    for f in ["up.sh", "down.sh", "outputs.sh"] {
        let text = script(&s, f);
        assert!(
            text.contains("[ ! -f \"$here/isoloom-instance\" ] || export TF_VAR_isoloom_instance=\"$(cat \"$here/isoloom-instance\")\"\n"),
            "{f}: {text}"
        );
    }
    // The apply's marker, for `status`.
    assert!(up.contains("echo $$ > \"$here/deploying\"\ntrap 'rm -f \"$here/deploying\"' EXIT\n"), "{up}");
    // Destroy copies the module only when its copy is gone (what the steps wrote there stays).
    let down = script(&s, "down.sh");
    assert!(down.contains("if [ ! -d \"$module\" ]; then\n  mkdir -p \"$module\"\n"), "{down}");
    assert!(down.contains("terraform -chdir=\"$module\" destroy"), "{down}");
    let outputs = script(&s, "outputs.sh");
    assert!(outputs.contains("[ -d \"$module\" ] || module=\"$src\"\n"), "{outputs}");
}

#[test]
fn before_steps_env_and_retries_reach_the_scripts() {
    let s = spec("  before: [keys.sh, 'scripts/my build.sh']\n  env: { AWS_REGION: us-east-1, QUOTED: \"it's\" }\n  apply_retries: 2\n");
    let up = script(&s, "up.sh");
    assert!(up.contains("export AWS_REGION='us-east-1'\n"), "{up}");
    assert!(up.contains("export QUOTED='it'\\''s'\n"), "{up}");
    assert!(up.contains("step 'keys.sh'\nstep 'scripts/my build.sh'\n"), "{up}");
    // The steps come after the copy and before init.
    let (copy, step, init) = (up.find("tar -C").unwrap(), up.find("step 'keys.sh'").unwrap(), up.find(" init ").unwrap());
    assert!(copy < step && step < init, "{up}");
    assert!(
        up.contains("  terraform -chdir=\"$module\" apply -auto-approve -input=false -state=\"$here/terraform.tfstate\" \"$@\" || rc=$?\n"),
        "{up}"
    );
    assert!(up.contains("[ \"$tries\" -le 2 ] || exit \"$rc\""), "{up}");
    for f in ["down.sh", "outputs.sh"] {
        assert!(script(&s, f).contains("export AWS_REGION='us-east-1'\n"), "{f}");
    }
    assert!(validate(&s).is_empty(), "{:?}", validate(&s));
}

#[test]
fn a_variable_holding_an_input_anywhere_stays_out_of_the_fixed_ones() {
    let s = spec("  vars: { allow: [\"{{ inputs.IP }}/32\"], region: eu-west-3 }\ninputs: [IP]\n");
    assert_eq!(script(&s, "terraform.tfvars.json"), "{\n  \"region\": \"eu-west-3\"\n}\n");
    let snapshot = isoloom_core::resolved::resolve(&s);
    assert_eq!(snapshot["cloud"]["inputs"], serde_json::json!(["IP"]));
}

#[test]
fn new_keys_are_validated() {
    let bad = spec(
        "  env: { 'AWS-REGION': x, 1X: y, here: z, PATH: /bin, TF_DATA_DIR: d, ISOLOOM_PROJECT: p, TF_VAR_isoloom_instance: i }\n  apply_retries: 9\n  requires: [' ']\n",
    );
    let found: Vec<String> = validate(&bad).iter().map(ToString::to_string).collect();
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
}

#[test]
fn a_before_step_must_exist_in_the_project() {
    let dir = std::env::temp_dir().join(format!("isoloom-cloud-before-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tf")).unwrap();
    std::fs::write(dir.join("tf/main.tf"), "").unwrap();
    let s = spec("  before: [keys.sh, ../out.sh]\n");
    let found: Vec<String> = isoloom_core::validate_files(&s, &dir).iter().map(ToString::to_string).collect();
    assert!(found.iter().any(|p| p.contains("cloud.before[0]") && p.contains("doesn't exist")), "{found:?}");
    assert!(
        found.iter().any(|p| p.contains("cloud.before[1]") && p.contains("inside the project")),
        "{found:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
