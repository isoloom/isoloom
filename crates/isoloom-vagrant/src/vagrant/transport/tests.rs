//! The transport's unit tests: which provisioners each planned step becomes.

use std::fmt::Write as _;

use isoloom_plan::Environment;
use isoloom_plan::lifecycle::Step;
use isoloom_spec::model::Target;

use super::{Os, provisioners, step_provisioners};
use crate::ruby::Ruby;

const SPEC: &str = r"
version: 1
name: t
inputs: [TOKEN]
networks:
  front: { cidr: 10.84.0.0/24 }
  back: { cidr: 10.85.0.0/24, gateway: fw, internet: false }
machines:
  fw:
    networks: { front: 2, back: 1 }
    vm: { os: debian@12, provision: [fw.sh] }
  app:
    networks: { back: 10 }
    inputs: [TOKEN]
    depends_on: [dc]
    ports: [{ port: 80 }]
    vm: { os: debian@12, provision: [a.sh, reboot, site.yml] }
  dc:
    networks: { front: 20 }
    ports: [{ port: 389 }]
    inputs: [TOKEN]
    vm: { os: windows-server@2019, provision: [dc.ps1] }
";

fn env() -> Environment {
    let spec = isoloom_spec::parse(SPEC).expect("the test spec parses");
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::Vagrant, None)
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
    assert!(s[default..].contains("systemctl enable isoloom-routes.service"), "kept across reboots: {s}");
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
    let inputs = &s[s.find("name: \"inputs\"").expect("an inputs step")..];
    assert!(
        inputs.starts_with("name: \"inputs\", env: INPUTS.slice(\"TOKEN\"), inline: <<~'SH'\n"),
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
    let start = s.find("umask 077").expect("the inputs script");
    let script: String = s[start..]
        .lines()
        .take(3)
        .fold(String::new(), |mut s, l| {
            let _ = writeln!(s, "{}", l.trim_start());
            s
        })
        .replace("/tmp/isoloom-inputs.env", "\"$OUT\"");
    let dir = std::env::temp_dir().join(format!("isoloom-inputs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let out = dir.join("inputs.env");
    let tricky = "it's $(not run) \"quoted\"";
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{script}set -a; . \"$OUT\"; set +a; printf %s \"$TOKEN\" > \"$OUT.read\""))
        .env("OUT", &out)
        .env("TOKEN", tricky)
        .status()
        .expect("sh runs");
    let read = std::fs::read_to_string(dir.join("inputs.env.read")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
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
    assert_eq!(names(&s), ["project", "hosts", "routes", "dc.ps1", "ready"], "{s}");
    assert!(s.contains("route -p add 10.85.0.0 MASK 255.255.255.0 10.84.0.2"), "through the gateway: {s}");
    assert!(s.contains("-File 'dc.ps1'") && s.contains("env: INPUTS.slice(\"TOKEN\")"), "{s}");
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
