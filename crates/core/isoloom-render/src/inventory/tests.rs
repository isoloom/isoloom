use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use super::*;
use crate::testing;

const SPEC: &str = r"
version: 1
name: inv
networks:
  lab: { cidr: 10.9.0.0/24 }
  back: { cidr: 10.8.0.0/24 }
groups:
  servers: { members: [web, dc, box] }
machines:
  web:
    networks: { back: 10, lab: 10 }
    os: debian@12
  dc:
    networks: { lab: 20 }
    os: windows-server@2022
  box:
    networks: { lab: 30 }
    app: { image: alpine }
provision:
  - ansible: site.yml
    groups: { dcs: [dc], boxes: [box] }
    host_vars:
      web: { code: '10', quote: it's, n: 3, on: true, none: null, l: [x, 1], m: { k: v } }
overrides:
  vagrant: { machines: { dc: { box: some/box, winrm: ssl } } }
  external:
    machines:
      web: { ssh: { address: web.example.org, user: ops, port: 2222, key: ~/my keys/id } }
      dc: { ssh: { address: 192.168.1.20 } }
";

fn env(spec: &str, target: Target) -> Environment {
    testing::env(spec, target)
}

/// The lines of section `[name]`, up to the next section (its blank line kept).
fn section<'a>(inv: &'a str, name: &str) -> &'a str {
    let (_, rest) = inv.split_once(&format!("[{name}]\n")).unwrap_or_else(|| panic!("[{name}] in\n{inv}"));
    rest.split_inclusive("\n[").next().map_or(rest, |s| s.strip_suffix('[').unwrap_or(s))
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
    let inv = render(&env(SPEC, Target::Vagrant), &Access::vagrant());
    assert_eq!(section(&inv, "dcs"), "dc\n\n");
    assert_eq!(section(&inv, "boxes"), "\n\n");
    assert_eq!(section(&inv, "servers"), "web\ndc\n");
    assert!(inv.ends_with("[servers]\nweb\ndc\n"), "{inv}");
}

#[test]
fn host_vars_keep_their_python_types_on_the_host_line() {
    let inv = render(&env(SPEC, Target::Vagrant), &Access::vagrant());
    let web = section(&inv, "linux").lines().next().unwrap();
    assert_eq!(
        web,
        r#"web ansible_host=10.8.0.10 code='"10"' quote='"it'"'"'s"' n='3' on='True' none='None' l='["x", 1]' m='{"k": "v"}'"#
    );
}

#[test]
fn windows_hosts_get_their_winrm_and_the_windows_vars() {
    let inv = render(&env(SPEC, Target::Vagrant), &Access::vagrant());
    assert_eq!(
        section(&inv, "windows"),
        "dc ansible_host=10.9.0.20 ansible_port=5986 ansible_winrm_scheme=https ansible_winrm_transport=ntlm\n\n"
    );
    assert!(section(&inv, "windows:vars").starts_with("ansible_user=vagrant\nansible_password=vagrant\nansible_connection=winrm\n"));
    assert_eq!(
        section(&inv, "linux:vars"),
        "ansible_user=vagrant\nansible_password=vagrant\nansible_become=true\n\n"
    );
}

#[test]
fn no_windows_vars_without_a_windows_host() {
    let linux_only = "version: 1\nname: l\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    os: debian@12\n";
    let inv = render(&env(linux_only, Target::Vagrant), &Access::vagrant());
    assert_eq!(
        inv,
        "[linux]\na ansible_host=10.9.0.10\n\n[windows]\n\n[linux:vars]\nansible_user=vagrant\nansible_password=vagrant\nansible_become=true\n"
    );
}

#[test]
fn external_hosts_carry_their_user_port_and_quoted_key() {
    let inv = render(&env(SPEC, Target::External), &Access::external());
    assert!(
        inv.starts_with("[linux]\nweb ansible_host=web.example.org ansible_user=ops ansible_port=2222 ansible_ssh_private_key_file='~/my keys/id' code="),
        "{inv}"
    );
    assert_eq!(section(&inv, "windows"), "dc ansible_host=192.168.1.20\n\n");
    assert_eq!(section(&inv, "windows:vars"), "ansible_connection=ssh\nansible_shell_type=powershell\n\n");
    assert_eq!(section(&inv, "linux:vars"), "ansible_become=true\n\n");
}

#[test]
fn proxmox_keeps_the_groups_and_gives_windows_its_winrm_account() {
    let inv = render(&env(SPEC, Target::Proxmox), &Access::proxmox("pw"));
    assert!(section(&inv, "linux").starts_with("web ansible_host=10.8.0.10 ansible_user=isoloom code="));
    assert_eq!(section(&inv, "windows"), "dc ansible_host=10.9.0.20\n\n");
    assert!(
        section(&inv, "windows:vars").starts_with("ansible_user=isoloom\nansible_password=pw\nansible_connection=winrm\n"),
        "{inv}"
    );
    assert_eq!(section(&inv, "servers"), "web\ndc\n");
}

#[test]
fn a_cloud_names_each_linux_user_and_the_windows_password() {
    let access = Access::cloud(
        |os| if os == "debian-12" { "admin".into() } else { "ubuntu".into() },
        "${random_password.windows.result}",
        WinrmAuth::Basic,
    );
    let inv = render(&env(SPEC, Target::CloudVm), &access);
    assert!(section(&inv, "linux").starts_with("web ansible_host=10.8.0.10 ansible_user=admin code="));
    assert!(section(&inv, "windows:vars").contains("ansible_password=${random_password.windows.result}\n"));
    assert!(section(&inv, "linux:vars").starts_with("ansible_ssh_private_key_file=/etc/isoloom/id_ed25519\n"));
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
    let shown = format!("{:?}", Access::external());
    assert!(shown.starts_with("Access { linux: [], windows: [(\"ansible_connection\", \"ssh\")"), "{shown}");
}
