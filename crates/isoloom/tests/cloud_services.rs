//! `isoloom run`, `status` and `down` on cloud services, against a fake `terraform` on the PATH
//! that records its calls and writes a state: the module's copy, the `before` steps, `env`,
//! `apply_retries`, `requires`, and `status` while the first apply runs.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A stand-in for Terraform: logs `<cwd>|<args>`, writes a lock file at init, and at apply
/// counts its applies, waits for `$FAKE_TF_WAIT` to exist when set, fails until the
/// `$FAKE_TF_FAIL_UNTIL`th apply, then writes a state with one resource.
const FAKE_TERRAFORM: &str = r#"#!/bin/sh
echo "$PWD|$*|${TF_VAR_isoloom_instance:-}" >> "$FAKE_TF_DIR/log"
case "$1" in version) echo '{"terraform_version":"1.9.0"}'; exit 0;; esac
dir="${1#-chdir=}"; shift
state=
for a; do case "$a" in -state=*) state="${a#-state=}";; esac; done
case "$1" in
  init) touch "$dir/.terraform.lock.hcl" ;;
  apply)
    n=$(cat "$FAKE_TF_DIR/applies" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$FAKE_TF_DIR/applies"
    if [ -n "${FAKE_TF_WAIT:-}" ]; then
      echo '{"resources":[{"type":"x"}]}' > "$state"
      while [ ! -f "$FAKE_TF_WAIT" ]; do sleep 0.1; done
    fi
    [ "$n" -ge "${FAKE_TF_FAIL_UNTIL:-1}" ] || { echo "apply failed" >&2; exit 3; }
    echo '{"resources":[{"type":"x"}]}' > "$state" ;;
  destroy) echo '{"resources":[]}' > "$state" ;;
  output) echo '{}' ;;
esac
"#;

/// A stand-in for the aws CLI: logs its arguments, prints `$FAKE_AWS_ARNS` (else `[]`).
const FAKE_AWS: &str = r#"#!/bin/sh
echo "$*" >> "$FAKE_TF_DIR/aws.log"
echo "${FAKE_AWS_ARNS:-[]}"
"#;

struct Lab {
    root: PathBuf,
    project: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    tf: PathBuf,
}

impl Lab {
    fn new(name: &str, cloud_fields: &str) -> Lab {
        let root = std::env::temp_dir().join(format!("isoloom-cloud-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (project, home, bin, tf) = (root.join("project"), root.join("home"), root.join("bin"), root.join("tf"));
        for d in [&project.join("terraform"), &home, &bin, &tf] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(project.join("terraform/main.tf"), "# a module\n").unwrap();
        std::fs::write(
            project.join("isoloom.yml"),
            format!("version: 1\nname: fake-cloud\ncloud:\n  provider: aws\n  terraform: terraform\n{cloud_fields}"),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        for (name, script) in [("terraform", FAKE_TERRAFORM), ("aws", FAKE_AWS)] {
            let fake = bin.join(name);
            std::fs::write(&fake, script).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Lab { root, project, home, bin, tf }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_isoloom"));
        let path = format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap_or_default());
        c.args(args)
            .arg(&self.project)
            .env("ISOLOOM_HOME", &self.home)
            .env("PATH", path)
            .env("AWS_ACCESS_KEY_ID", "fake")
            .env("FAKE_TF_DIR", &self.tf)
            .env_remove("FAKE_TF_WAIT")
            .env_remove("FAKE_AWS_ARNS")
            .env_remove("FAKE_TF_FAIL_UNTIL");
        c
    }

    fn isoloom(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn status_state(&self) -> Option<String> {
        let o = Command::new(env!("CARGO_BIN_EXE_isoloom"))
            .args(["status", "--json"])
            .env("ISOLOOM_HOME", &self.home)
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).ok()?;
        v["environments"]
            .as_array()?
            .iter()
            .find(|e| e["name"] == "fake-cloud")
            .and_then(|e| e["state"].as_str().map(String::from))
    }

    fn out(&self) -> PathBuf {
        self.project.join(".isoloom/cloud-services")
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn stdout_has(o: &Output, s: &str) -> bool {
    String::from_utf8_lossy(&o.stdout).contains(s)
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

#[test]
fn run_applies_a_copy_after_the_before_steps_and_retries() {
    let lab = Lab::new(
        "run",
        "  vars: { allow: [\"{{ inputs.PLAYER_IP }}/32\"], n: 2 }\n  before: [keys.sh]\n  env: { AWS_REGION: us-east-1 }\n  apply_retries: 1\ninputs: [PLAYER_IP]\n",
    );
    // The step writes a file where it runs, and what it was given.
    std::fs::write(lab.project.join("keys.sh"), "touch key.pub\necho \"$AWS_REGION $ISOLOOM_PROJECT\" > step.txt\n").unwrap();
    let o = lab.isoloom(&["run", "cloud-services"]);
    assert!(text(&o).contains("set PLAYER_IP in the environment"), "{}", text(&o));
    let o = lab
        .command(&["run", "cloud-services"])
        .env("FAKE_TF_FAIL_UNTIL", "2")
        .env("PLAYER_IP", "203.0.113.7")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    // The input, in its place in the list, kept for `down`; the fixed values apart.
    let inputs: serde_json::Value = serde_json::from_str(&read(&lab.out().join("inputs.tfvars.json"))).unwrap();
    assert_eq!(inputs, serde_json::json!({ "allow": ["203.0.113.7/32"] }));
    let fixed: serde_json::Value = serde_json::from_str(&read(&lab.out().join("terraform.tfvars.json"))).unwrap();
    assert_eq!(fixed, serde_json::json!({ "n": 2 }));
    assert!(text(&o).contains("applying again (1 of 1)"), "{}", text(&o));
    let module = lab.out().join("module");
    assert!(module.join("main.tf").is_file());
    assert!(module.join("key.pub").is_file(), "the step ran in the copy");
    assert!(module.join(".terraform.lock.hcl").is_file(), "the lock file lands in the copy");
    assert!(!lab.project.join("terraform/.terraform.lock.hcl").exists(), "the module is untouched");
    assert!(!lab.project.join("terraform/key.pub").exists());
    let project = lab.project.canonicalize().unwrap();
    assert_eq!(read(&module.join("step.txt")).trim(), format!("us-east-1 {}", project.display()));
    assert_eq!(read(&lab.tf.join("applies")).trim(), "2");
    assert!(!lab.out().join("deploying").exists(), "the marker goes once applied");
    assert_eq!(lab.status_state().as_deref(), Some("deployed (1 resources)"));

    let o = lab.isoloom(&["down", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    let log = read(&lab.tf.join("log"));
    // The canonical path: on macOS the temporary folder (/var) is a link to /private/var.
    let module = module.canonicalize().unwrap();
    assert!(log.lines().any(|l| l.contains(&format!("-chdir={} destroy", module.display()))), "{log}");
    assert_eq!(lab.status_state(), None);
}

#[test]
fn run_refuses_without_what_the_module_requires() {
    let lab = Lab::new("requires", "  requires: [isoloom-no-such-tool, \"touch \\\"$FAKE_TF_DIR/ran\\\"\"]\n");
    let o = lab.isoloom(&["run", "cloud-services"]);
    assert!(!o.status.success());
    assert!(text(&o).contains("`isoloom-no-such-tool` isn't installed"), "{}", text(&o));
    assert!(lab.tf.join("ran").exists(), "run checks the shell commands");
    std::fs::remove_file(lab.tf.join("ran")).unwrap();
    assert!(!lab.tf.join("applies").exists(), "nothing applied");
    // `doctor` says the same for the spec in the folder, but never runs its shell commands.
    let o = lab.isoloom(&["doctor"]);
    assert!(text(&o).contains("✗ the spec's requires"), "{}", text(&o));
    assert!(text(&o).contains("`isoloom-no-such-tool` isn't installed"), "{}", text(&o));
    assert!(!lab.tf.join("ran").exists(), "doctor ran the spec's command");
    let o = lab.isoloom(&["doctor", "--json"]);
    assert!(stdout_has(&o, "(checked at `isoloom run`)"), "{}", text(&o));
}

#[test]
fn status_says_deploying_while_the_first_apply_runs() {
    let lab = Lab::new("deploying", "");
    let gate = lab.tf.join("go");
    let mut child = lab
        .command(&["run", "cloud-services"])
        .env("FAKE_TF_WAIT", &gate)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut seen = None;
    while std::time::Instant::now() < deadline {
        seen = lab.status_state();
        // "deploying" alone until the fake apply has written its state: wait for the count.
        if seen.as_deref().is_some_and(|s| s.starts_with("deploying (")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::fs::write(&gate, "").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(seen.as_deref(), Some("deploying (1 resources so far)"));
    assert_eq!(lab.status_state().as_deref(), Some("deployed (1 resources)"));
}

#[test]
fn a_renamed_module_file_leaves_the_copy_and_a_failed_apply_keeps_its_code() {
    let lab = Lab::new("rename", "  apply_retries: 1\n");
    let o = lab.isoloom(&["run", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    std::fs::rename(lab.project.join("terraform/main.tf"), lab.project.join("terraform/site.tf")).unwrap();
    // Every apply fails from here: the last failure's code is what `run` reports.
    let o = lab.command(&["run", "cloud-services"]).env("FAKE_TF_FAIL_UNTIL", "99").output().unwrap();
    assert!(!o.status.success());
    assert!(text(&o).contains("exit status: 3"), "{}", text(&o));
    assert_eq!(read(&lab.tf.join("applies")).trim(), "3");
    let module = lab.out().join("module");
    assert!(module.join("site.tf").is_file());
    assert!(!module.join("main.tf").exists(), "the old name is still applied");
}

#[test]
fn down_lists_what_is_left_with_the_deployments_tag() {
    let lab = Lab::new("leftovers", "  vars: { region: eu-west-3 }\n");
    let o = lab.isoloom(&["run", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    let id = read(&lab.out().join("isoloom-instance")).trim().to_string();
    assert!(id.starts_with("isoloom-fake-cloud-") && id.len() == "isoloom-fake-cloud-".len() + 8, "{id}");
    // Terraform gets it as `var.isoloom_instance`.
    let log = read(&lab.tf.join("log"));
    assert!(log.lines().any(|l| l.contains(" apply ") && l.ends_with(&format!("|{id}"))), "{log}");

    let arn = "arn:aws:logs:eu-west-3:123456789012:log-group:/aws/lambda/x";
    let o = lab
        .command(&["down", "cloud-services"])
        .env("FAKE_AWS_ARNS", format!("[\"{arn}\"]"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains(&format!("tagged isoloom-instance={id} (1;")), "{}", text(&o));
    assert!(text(&o).contains(&format!("  {arn}")), "{}", text(&o));
    let aws = read(&lab.tf.join("aws.log"));
    assert!(
        aws.contains(&format!(
            "resourcegroupstaggingapi get-resources --tag-filters Key=isoloom-instance,Values={id} "
        )),
        "{aws}"
    );
    assert!(aws.trim_end().ends_with("--region eu-west-3"), "{aws}");
    // Kept while something is left, forgotten once nothing is.
    assert!(lab.out().join("isoloom-instance").exists());
    let o = lab.isoloom(&["down", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(!text(&o).contains("Still in the cloud"), "{}", text(&o));
    assert!(!lab.out().join("isoloom-instance").exists());
}

#[test]
fn leftovers_use_the_applied_region_and_keep_the_name_when_it_is_unknown() {
    // The region from an input: the value applied, read back at `down`.
    let lab = Lab::new("region-input", "  vars: { region: \"{{ inputs.REGION }}\" }\ninputs: [REGION]\n");
    let o = lab.command(&["run", "cloud-services"]).env("REGION", "ap-south-1").output().unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let o = lab.isoloom(&["down", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    let aws = read(&lab.tf.join("aws.log"));
    assert!(aws.trim_end().ends_with("--region ap-south-1"), "{aws}");
    assert!(!lab.out().join("isoloom-instance").exists());

    // No region anywhere: the CLI's default is searched, and an empty answer forgets nothing.
    let lab = Lab::new("region-unknown", "");
    let o = lab.isoloom(&["run", "cloud-services"]);
    assert!(o.status.success(), "{}", text(&o));
    let o = lab
        .command(&["down", "cloud-services"])
        .env_remove("AWS_REGION")
        .env_remove("AWS_DEFAULT_REGION")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(!read(&lab.tf.join("aws.log")).contains("--region"));
    assert!(text(&o).contains("the name is kept"), "{}", text(&o));
    assert!(lab.out().join("isoloom-instance").exists());
}
