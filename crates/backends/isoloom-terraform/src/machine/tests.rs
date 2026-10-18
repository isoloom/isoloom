//! The VM core on small plans: the transports split the steps at a reboot, the hooks land where
//! they say.

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Expression, Module, Template, reference, string};
use isoloom_spec::model::Target;

use super::Remote;
use super::cloud_init::{self, CloudConfig};
use super::linux::{self, Hooks};
use super::provisioner::Provisioner;
use crate::testing::environment;

const SPEC: &str = "
version: 1
name: core
inputs: { TOKEN: { sensitive: false } }
networks:
  lab: { cidr: 10.9.0.0/24, internet: false }
machines:
  a:
    networks: { lab: 10 }
    ports: { web: 80 }
    environment: { TOKEN: { input: TOKEN } }
    os: debian@12
    app: { steps: [{ run: one.sh }, reboot, { run: two.sh }] }
  b:
    networks: { lab: 11 }
    depends_on: [a]
    os: debian@12
    app: { steps: [{ run: b.sh }] }
";

fn env() -> Environment {
    environment(SPEC, Target::CloudVm)
}

fn text(e: &Expression) -> String {
    e.to_string()
}

/// Machine `name`'s set-up over SSH as `admin` at `host`, with `hooks`.
fn provisioner(env: &Environment, name: &str, host: Expression, hooks: Hooks) -> Provisioner {
    linux::provisioner(linux::Machine {
        remote: Remote {
            env,
            name,
            plan: &env.machines[name],
            replace_on: string("id"),
            host,
        },
        user: "admin".to_owned(),
        hooks,
    })
}

#[test]
fn remote_exec_splits_at_the_reboot_and_runs_the_hooks() {
    let env = env();
    let hooks = Hooks {
        at_boot: vec![Template::text("echo boot")],
        after_reboot: vec![Template::text("echo again")],
        before_egress: vec![Template::text("echo redirect")],
        skip: |s| matches!(s, Step::Routes { .. }),
    };
    let p = provisioner(&env, "a", string("host"), hooks);
    assert_eq!(p.parts.len(), 2);
    let first: Vec<String> = p.parts[0].iter().map(text).collect();
    let second: Vec<String> = p.parts[1].iter().map(text).collect();
    assert!(first.iter().any(|c| c.contains("echo boot")) && first.iter().any(|c| c.contains("one.sh")));
    assert!(first.last().unwrap().contains("boot-id"), "{first:?}");
    assert!(
        second[1].contains("did not restart") && second[2].contains("echo again") && second[3].contains("echo boot"),
        "{second:?}"
    );
    let redirect = second.iter().position(|c| c.contains("echo redirect")).unwrap();
    let egress = second.iter().position(|c| c.contains("isoloom-egress")).unwrap();
    assert!(second.iter().position(|c| c.contains("two.sh")).unwrap() < redirect && redirect < egress);
    // The inputs are uploaded with the project; `b` waits for `a`'s set-up.
    assert_eq!(p.uploads.len(), 2);
    let b = linux::depends_on(&env, &env.machines["b"]);
    assert_eq!(b.iter().map(text).collect::<Vec<_>>(), ["terraform_data.a"]);
}

#[test]
fn cloud_init_runs_the_rest_at_the_next_boot() {
    let env = env();
    let mut c = CloudConfig::new("a", string("users"));
    cloud_init::machine(&env, ("a", &env.machines["a"]), &mut c);
    assert!(c.restart);
    assert!(c.runcmd.iter().any(|r| r.contains("one.sh")) && !c.runcmd.iter().any(|r| r.contains("two.sh")));
    assert!(c.runcmd.last().unwrap().contains("next-boot"));
    // The inputs file, the unit, its script and one part.
    assert_eq!(c.files.len(), 4);
    let doc = text(&c.document());
    assert!(doc.starts_with("format(\"#cloud-config\\n%s\", yamlencode({"), "{doc}");
    assert!(doc.contains("power_state"), "{doc}");
}

#[test]
fn a_cloud_config_leaves_its_empty_lists_out() {
    let empty = text(&CloudConfig::new("a", string("users")).document());
    for key in ["packages", "bootcmd", "write_files", "runcmd"] {
        assert!(!empty.contains(key), "{key}: {empty}");
    }
    let mut c = CloudConfig::new("a", string("users"));
    c.packages.push("curl".into());
    c.runcmd.push("true".into());
    let some = text(&c.document());
    assert!(
        some.contains("packages = [\"curl\"]") && some.contains("runcmd = [\"true\"]") && !some.contains("write_files"),
        "{some}"
    );
}

#[test]
fn a_machine_waiting_for_secrets_announces_its_address_first() {
    let spec = format!("{SPEC}secrets:\n  db: {{ env: DB }}\n").replace(
        "    environment: { TOKEN: { input: TOKEN } }\n",
        "    environment: { TOKEN: { input: TOKEN } }\n    files: [{ path: /run/secrets/db, secret: db }]\n",
    );
    let env = environment(&spec, Target::CloudVm);
    let machine = |name| provisioner(&env, name, reference("aws_instance.a.public_ip"), Hooks::default());
    let a = machine("a");
    assert_eq!(a.announce.as_deref(), Some("a"));
    assert!(machine("b").announce.is_none());
    let first: Vec<String> = a.parts[0].iter().map(text).collect();
    assert!(first.iter().any(|c| c.contains("/var/lib/isoloom/secrets/.delivered")), "{first:?}");
    let mut m = Module::new("");
    m.block(a.block());
    let hcl = m.render().unwrap();
    let announce =
        hcl.find("mkdir -p \\\"${path.module}/.isoloom-reach\\\" && echo \\\"admin@${aws_instance.a.public_ip}\\\" > \\\"${path.module}/.isoloom-reach/a\\\"");
    let upload = hcl.find("provisioner \"file\"");
    assert!(announce.is_some() && announce < upload, "{hcl}");
}

/// A tracked step run by a real `sh`, its ready file at `ready`: its exit status and what the
/// ready file says after.
#[cfg(unix)]
fn run_tracked(command: &str, ready: &std::path::Path) -> (bool, String) {
    let script = cloud_init::tracked("step", command).replace(cloud_init::READY_FILE, &ready.display().to_string());
    let status = std::process::Command::new("sh").arg("-c").arg(&script).status().expect("sh runs");
    (status.success(), std::fs::read_to_string(ready).unwrap_or_default())
}

#[cfg(unix)]
#[test]
fn a_tracked_step_marks_its_progress_best_effort_and_its_failure_always() {
    let dir = std::env::temp_dir().join(format!("isoloom-tracked-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let ready = dir.join("ready");
    // It runs: the progress is marked, and stays the last word.
    assert_eq!(run_tracked("true", &ready), (true, "running: step\n".to_owned()));
    // It fails: the failure is marked and the set-up ends there.
    assert_eq!(run_tracked("false", &ready), (false, "failed: step\n".to_owned()));
    // The progress can't be written (no such folder): the step still runs, and succeeds.
    let ran = dir.join("ran");
    let (ok, _) = run_tracked(&format!("touch {}", ran.display()), &dir.join("missing/ready"));
    assert!(ok && ran.exists(), "a progress mark that can't be written doesn't skip the step");
    std::fs::remove_dir_all(&dir).ok();
}
