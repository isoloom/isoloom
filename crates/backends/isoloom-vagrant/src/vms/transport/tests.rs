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

/// What `write` writes, as the Vagrantfile has it.
fn text(write: impl FnOnce(&mut Ruby)) -> String {
    let mut r = Ruby::new();
    write(&mut r);
    r.finish()
}

/// Each provisioner `write` writes, its name and its statement (its script included), in order.
fn provisioned(write: impl FnOnce(&mut Ruby)) -> Vec<(String, String)> {
    let s = text(write);
    isoloom_test_support::ruby_blocks(&s)
        .statements
        .iter()
        .filter_map(|p| {
            let name = p.lines().next()?.split("name: \"").nth(1)?.split('"').next()?;
            Some((name.to_owned(), p.clone()))
        })
        .collect()
}

/// The statement of the provisioner named `name`.
fn named<'p>(provisioners: &'p [(String, String)], name: &str) -> &'p str {
    let found: Vec<&str> = provisioners.iter().filter(|(n, _)| n == name).map(|(_, s)| s.as_str()).collect();
    match found.as_slice() {
        [one] => one,
        _ => panic!("{} provisioners named {name}: {provisioners:?}", found.len()),
    }
}

/// The provisioner names, in order.
fn names(s: &str) -> Vec<&str> {
    s.lines().filter_map(|l| l.split("name: \"").nth(1)?.split('"').next()).collect()
}

#[test]
fn the_steps_run_in_the_plans_order() {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux, |_| {}));
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

/// The default route through the gateway is set once the steps ran (after the playbook), by its
/// own provisioner alone, and kept across reboots.
#[test]
fn the_default_route_comes_once_provisioned() {
    let p = provisioned(|r| provisioners(r, &env(), "app", Os::Linux, |_| {}));
    let at = |name: &str| p.iter().position(|(n, _)| n == name);
    assert!(at("site.yml") < at("default route") && at("site.yml").is_some(), "{p:?}");
    let setting: Vec<&str> = p
        .iter()
        .filter(|(_, s)| s.contains("ip route replace default via 10.85.0.1"))
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(setting, ["default route"]);
    assert!(
        named(&p, "default route").contains("systemctl enable isoloom-routes.service"),
        "kept across reboots: {p:?}"
    );
}

#[test]
fn a_gateway_forwards() {
    let s = text(|r| provisioners(r, &env(), "fw", Os::Linux, |_| {}));
    assert!(names(&s).contains(&"forwarding"), "{s}");
}

#[test]
fn a_reboot_is_vagrants_own_between_a_mark_and_its_check() {
    let s = text(|r| step_provisioners(r, &env(), "app", (0, &Step::Reboot), Os::Linux));
    assert!(s.contains("name: \"reboot\", reboot: true\n"), "{s}");
    assert!(s.contains("cp /proc/sys/kernel/random/boot_id") && s.contains("did not restart"), "{s}");
}

#[test]
fn inputs_come_from_the_hosts_environment_into_the_file_the_steps_read() {
    let env = env();
    let s = text(|r| provisioners(r, &env, "app", Os::Linux, |_| {}));
    let inputs = s
        .split_once("name: \"inputs\"")
        .map(|(_, rest)| format!("name: \"inputs\"{rest}"))
        .expect("an inputs step");
    assert!(
        inputs.starts_with("name: \"inputs\", env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }, inline: <<~'SH'\n"),
        "{inputs}"
    );
    assert!(
        inputs.contains("v=\"$(printenv 'TOKEN' || true)\"; if [ -n \"$v\" ] || [ ! -e '/var/lib/isoloom/inputs/TOKEN' ]")
            && inputs.contains("/var/lib/isoloom/inputs.env"),
        "kept for the reboot: {inputs}"
    );
    assert!(s.contains(". /var/lib/isoloom/inputs.env"), "the steps read the kept file: {s}");
}

/// The inputs step's own lines (before the persisting step), as a script run in a test's folder
/// writing the inputs file at `$OUT` and keeping the values in `./kept`, then reading `TOKEN`
/// back from the file into `$OUT.read`.
fn inputs_script() -> String {
    let s = text(|r| provisioners(r, &env(), "app", Os::Linux, |_| {}));
    let (_, from_umask) = s.split_once("umask 077").expect("the inputs script");
    let lines: Vec<String> = format!("umask 077{from_umask}")
        .lines()
        .take_while(|l| !l.contains("unset v"))
        .map(|l| format!("{}\n", l.trim_start()))
        .collect();
    // Run from a file, in the test's folder, the inputs file at a relative path: on Windows, Git's
    // `sh` reads a `-c` script's backslashes before quotes otherwise than it was written, and a
    // Windows path has no `/`, so `.` would look it up on PATH.
    let script = lines
        .concat()
        .replace("/tmp/isoloom-inputs.env", "\"$OUT\"")
        .replace(super::values::KEPT_INPUTS_DIR, "./kept");
    format!("{script}set -a; . \"$OUT\"; set +a; printf %s \"$TOKEN\" > \"$OUT.read\"\n")
}

/// [`inputs_script`] run in `dir` with `TOKEN` set to `value` in base64 (unset: `None`): whether
/// it succeeded, and the value the steps read.
fn provision_inputs(dir: &std::path::Path, value: Option<&str>) -> (bool, String) {
    std::fs::write(dir.join("inputs.sh"), inputs_script()).expect("the script written");
    let mut posix = isoloom_render::test_support::posix_sh().expect("a POSIX shell, checked by the caller");
    posix.arg("inputs.sh").current_dir(dir).env("OUT", "./inputs.env").env_remove("TOKEN");
    if let Some(v) = value {
        posix.env("TOKEN", isoloom_spec::shell::base64(v.as_bytes()));
    }
    let ok = posix.status().expect("sh runs").success();
    (ok, std::fs::read_to_string(dir.join("inputs.env.read")).unwrap_or_default())
}

#[test]
fn the_inputs_file_quotes_every_value() {
    if isoloom_render::test_support::posix_sh().is_none() {
        return;
    }
    let dir = tempfile::Builder::new().prefix("isoloom-inputs-").tempdir().expect("a temporary folder");
    let tricky = "it's $(not run) \"quoted\" `nor this`";
    assert_eq!(provision_inputs(dir.path(), Some(tricky)), (true, tricky.to_owned()));
}

#[test]
fn a_provisioning_given_no_input_keeps_the_one_given_before() {
    if isoloom_render::test_support::posix_sh().is_none() {
        return;
    }
    let dir = tempfile::Builder::new().prefix("isoloom-inputs-kept-").tempdir().expect("a temporary folder");
    let dir = dir.path();
    assert_eq!(provision_inputs(dir, None), (true, String::new()), "unset the first time: empty");
    assert_eq!(provision_inputs(dir, Some("first")), (true, "first".to_owned()));
    assert_eq!(provision_inputs(dir, None), (true, "first".to_owned()), "given none: kept");
    assert_eq!(provision_inputs(dir, Some("second")), (true, "second".to_owned()));
}

#[test]
fn a_linux_machine_waits_for_a_windows_one_by_address() {
    let p = provisioned(|r| provisioners(r, &env(), "app", Os::Linux, |_| {}));
    let wait = named(&p, "wait for dc");
    assert!(wait.contains("</dev/tcp/10.84.0.20/389"), "{wait}");
}

#[test]
fn windows_runs_powershell_with_its_inputs() {
    let s = text(|r| provisioners(r, &env(), "dc", Os::Windows, |_| {}));
    assert_eq!(names(&s), ["project", "hosts", "routes", "inputs", "dc.ps1", "ready"], "{s}");
    let p = provisioned(|r| provisioners(r, &env(), "dc", Os::Windows, |_| {}));
    let routes = named(&p, "routes");
    assert!(
        routes.contains("route -p add 10.85.0.0 MASK 255.255.255.0 10.84.0.2"),
        "through the gateway: {routes}"
    );
    let run = named(&p, "dc.ps1");
    assert!(run.contains("env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }"), "{run}");
    assert!(run.contains("FromBase64String"), "decoded before the script reads it: {run}");
    assert!(!s.contains("/etc/"), "no Linux step on Windows: {s}");
}

#[test]
fn egress_survives_a_reboot_and_a_machine_without_trunks_gets_no_trunk_step() {
    let env = env();
    let s = text(|r| step_provisioners(r, &env, "app", (0, &Step::Egress), Os::Linux));
    assert!(s.contains("name: \"no internet\"") && s.contains("ExecStart=-/etc/isoloom/egress.sh"), "{s}");
    assert!(
        text(|r| step_provisioners(r, &env, "app", (0, &Step::Trunks), Os::Linux)).is_empty(),
        "no trunk: nothing to run"
    );
}
