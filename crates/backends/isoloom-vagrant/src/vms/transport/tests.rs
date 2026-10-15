//! The transport's unit tests: which provisioners each planned step becomes.

use isoloom_plan::Environment;
use isoloom_plan::lifecycle::Step;

use super::{Os, provisioners, step_provisioners};
use crate::ruby::Ruby;
use crate::testing::environment;

const SPEC: &str = r"
version: 1
name: t
inputs: { TOKEN: { sensitive: false } }
networks:
  front: { cidr: 10.84.0.0/24 }
  back: { cidr: 10.85.0.0/24, gateway: fw, internet: false }
machines:
  fw:
    networks: { front: 2, back: 1 }
    os: debian@12
    app: { steps: [{ run: fw.sh }] }
  app:
    networks: { back: 10 }
    environment: { TOKEN: { input: TOKEN } }
    depends_on: [dc]
    ports: { web: 80 }
    os: debian@12
    app: { steps: [{ run: a.sh }, reboot, { playbook: site.yml }] }
  dc:
    networks: { front: 20 }
    ports: { ldap: 389 }
    environment: { TOKEN: { input: TOKEN } }
    os: windows-server@2019
    app: { steps: [{ run: dc.ps1 }] }
";

fn env() -> Environment {
    environment(SPEC)
}

#[test]
fn the_test_spec_is_valid() {
    let lab = isoloom_plan::Lab::parse(SPEC).expect("the test spec parses");
    assert_eq!(lab.problems(), Vec::new());
}

/// What `write` writes, as the Vagrantfile has it.
fn text(write: impl FnOnce(&mut Ruby)) -> String {
    let mut r = Ruby::new();
    write(&mut r);
    r.finish()
}

/// The provisioner names, in order.
fn names(s: &str) -> Vec<&str> {
    s.lines().filter_map(|l| l.split("name: \"").nth(1)?.split('"').next()).collect()
}

#[test]
fn the_steps_run_in_the_plans_order() {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux));
    assert_eq!(
        names(&s),
        [
            "project",
            "hosts",
            "wait for dc",
            "inputs",
            "a.sh",
            "before reboot",
            "reboot",
            "after reboot",
            "site.yml",
            "default route",
            "ready"
        ],
        "{s}"
    );
}

#[test]
fn the_default_route_comes_once_provisioned() {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux));
    let playbook = s.find("name: \"site.yml\"").expect("the playbook");
    let default = s.find("name: \"default route\"").expect("the default route");
    assert!(playbook < default, "{s}");
    assert_eq!(s.matches("ip route replace default via 10.85.0.1").count(), 1, "{s}");
    let after_default = s.get(default..).unwrap_or_default();
    assert!(after_default.contains("systemctl enable isoloom-routes.service"), "kept across reboots: {s}");
}

#[test]
fn a_gateway_forwards() {
    let s = text(|r| provisioners(r, &env(), "fw", Os::Linux));
    assert!(names(&s).contains(&"forwarding"), "{s}");
}

#[test]
fn a_reboot_is_vagrants_own_between_a_mark_and_its_check() {
    let s = text(|r| step_provisioners(r, &env(), "app", &Step::Reboot, Os::Linux));
    assert!(s.contains("name: \"reboot\", reboot: true\n"), "{s}");
    assert!(s.contains("cp /proc/sys/kernel/random/boot_id") && s.contains("did not restart"), "{s}");
}

#[test]
fn inputs_come_from_the_hosts_environment_into_the_file_the_steps_read() {
    let env = env();
    let s = text(|r| provisioners(r, &env, "app", Os::Linux));
    let inputs = s
        .split_once("name: \"inputs\"")
        .map(|(_, rest)| format!("name: \"inputs\"{rest}"))
        .expect("an inputs step");
    assert!(
        inputs.starts_with("name: \"inputs\", env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }, inline: <<~'SH'\n"),
        "{inputs}"
    );
    assert!(
        inputs.contains("for k in TOKEN; do") && inputs.contains("/var/lib/isoloom/inputs.env"),
        "kept for the reboot: {inputs}"
    );
    assert!(s.contains(". /var/lib/isoloom/inputs.env"), "the steps read the kept file: {s}");
}

#[test]
fn the_inputs_file_quotes_every_value() {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux));
    let (_, from_umask) = s.split_once("umask 077").expect("the inputs script");
    let lines: Vec<String> = format!("umask 077{from_umask}")
        .lines()
        .take(3)
        .map(|l| format!("{}\n", l.trim_start()))
        .collect();
    // Run from a file, in the test's folder, the inputs file at a relative path: on Windows, Git's
    // `sh` reads a `-c` script's backslashes before quotes otherwise than it was written, and a
    // Windows path has no `/`, so `.` would look it up on PATH.
    let script = lines.concat().replace("/tmp/isoloom-inputs.env", "\"$OUT\"");
    let dir = std::env::temp_dir().join(format!("isoloom-inputs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let read_back = "set -a; . \"$OUT\"; set +a; printf %s \"$TOKEN\" > \"$OUT.read\"\n";
    std::fs::write(dir.join("inputs.sh"), format!("{script}{read_back}")).expect("the script written");
    let tricky = "it's $(not run) \"quoted\" `nor this`";
    let status = std::process::Command::new("sh")
        .arg("inputs.sh")
        .current_dir(&dir)
        .env("OUT", "./inputs.env")
        .env("TOKEN", isoloom_spec::shell::base64(tricky.as_bytes()))
        .status()
        .expect("sh runs");
    let read = std::fs::read_to_string(dir.join("inputs.env.read")).unwrap_or_default();
    std::fs::remove_dir_all(&dir).expect("the temporary folder goes");
    assert!(status.success());
    assert_eq!(read, tricky);
}

#[test]
fn a_linux_machine_waits_for_a_windows_one_by_address() {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux));
    assert!(s.contains("name: \"wait for dc\"") && s.contains("</dev/tcp/10.84.0.20/389"), "{s}");
}

#[test]
fn windows_runs_powershell_with_its_inputs() {
    let s = text(|r| provisioners(r, &env(), "dc", Os::Windows));
    assert_eq!(names(&s), ["project", "hosts", "routes", "inputs", "dc.ps1", "ready"], "{s}");
    assert!(s.contains("route -p add 10.85.0.0 MASK 255.255.255.0 10.84.0.2"), "through the gateway: {s}");
    let run = s.lines().find(|l| l.contains("-File 'dc.ps1'")).unwrap_or_default();
    assert!(run.contains("env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }"), "{run}");
    assert!(run.contains("FromBase64String"), "decoded before the script reads it: {run}");
    assert!(!s.contains("/etc/"), "no Linux step on Windows: {s}");
}

#[test]
fn routes_trunks_tc_and_egress_survive_a_reboot() {
    let env = env();
    let s = text(|r| step_provisioners(r, &env, "app", &Step::Egress, Os::Linux));
    assert!(s.contains("name: \"no internet\"") && s.contains("ExecStart=-/etc/isoloom/egress.sh"), "{s}");
    assert!(
        text(|r| step_provisioners(r, &env, "app", &Step::Trunks, Os::Linux)).is_empty(),
        "no trunk: nothing to run"
    );
}
