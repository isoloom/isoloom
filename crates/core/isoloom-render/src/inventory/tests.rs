use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use super::*;
use crate::testing;

use crate::test_support::{INVENTORY_SPEC as SPEC, section};

/// An access with one Linux group variable and nothing per host.
fn plain() -> Access<'static> {
    Access::new(vars(&[("ansible_user", "u")]), Vars::new(), |_| Vars::new())
}

fn env(spec: &str, target: Target) -> Environment {
    testing::env(spec, target)
}

#[test]
fn vms_are_hosts_at_their_first_address_and_containers_are_not() {
    let env = env(SPEC, Target::Vagrant);
    let hosts: Vec<(&str, String)> = hosts(&env).into_iter().map(|h| (h.name, h.address)).collect();
    assert_eq!(hosts, [("web", "10.8.0.10".to_owned()), ("dc", "10.9.0.20".to_owned())]);
}

#[test]
fn existing_hosts_are_reached_at_their_external_address() {
    let env = env(SPEC, Target::External);
    let names: Vec<String> = hosts(&env).into_iter().map(|h| format!("{} {}", h.name, h.address)).collect();
    assert_eq!(names, ["web web.example.org", "dc 192.168.1.20"]);
}

#[test]
fn groups_keep_only_the_hosts_and_stay_declared_when_empty() {
    let inv = render(&env(SPEC, Target::Vagrant), &plain());
    assert_eq!(section(&inv, "dcs"), "dc\n\n");
    assert_eq!(section(&inv, "boxes"), "\n\n");
    assert_eq!(section(&inv, "servers"), "web\ndc\n");
    assert!(inv.ends_with("[servers]\nweb\ndc\n"), "{inv}");
}

#[test]
fn host_vars_keep_their_python_types_on_the_host_line() {
    let inv = render(&env(SPEC, Target::Vagrant), &plain());
    let web = section(&inv, "linux").lines().next().unwrap();
    assert_eq!(
        web,
        r#"web ansible_host=10.8.0.10 code='"10"' quote='"it'"'"'s"' n='3' on='True' none='None' l='["x", 1]' m='{"k": "v"}'"#
    );
}

#[test]
fn no_windows_vars_without_a_windows_host() {
    let linux_only = "version: 1\nname: l\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    os: debian@12\n";
    let inv = render(&env(linux_only, Target::Vagrant), &plain());
    assert_eq!(
        inv,
        "[linux]\na ansible_host=10.9.0.10\n\n[windows]\n\n[linux:vars]\nansible_user=u\nansible_become=true\n"
    );
}

#[test]
fn host_values_are_quoted_only_when_the_split_needs_it() {
    assert_eq!(host_value("~/.ssh/id_ed25519"), "~/.ssh/id_ed25519");
    assert_eq!(host_value("a b"), "'a b'");
    assert_eq!(host_value("it's"), r#"'it'"'"'s'"#);
    assert_eq!(host_value(""), "''");
    assert_eq!(host_value("x#y"), "'x#y'");
}

#[test]
fn an_access_shows_its_group_variables() {
    let shown = format!("{:?}", Access::new(Vars::new(), vars(&WINDOWS_OVER_SSH), |_| Vars::new()));
    assert!(shown.starts_with("Access { linux: [], windows: [(\"ansible_connection\", \"ssh\")"), "{shown}");
}

#[test]
fn each_host_gets_its_access_s_own_variables_after_its_address() {
    let access = Access::new(Vars::new(), vars(&[("ansible_connection", "winrm")]), |h| {
        h.machine.winrm.map(winrm_vars).unwrap_or_default()
    });
    let inv = render(&env(SPEC, Target::Vagrant), &access);
    assert_eq!(
        section(&inv, "windows"),
        "dc ansible_host=10.9.0.20 ansible_port=5986 ansible_winrm_scheme=https ansible_winrm_transport=ntlm\n\n"
    );
    assert_eq!(section(&inv, "windows:vars"), "ansible_connection=winrm\n\n");
}
