//! The PowerShell rendering: each step kind's call, its arguments single-quoted.

use isoloom_spec::model::Target;

use super::*;
use crate::testing;

/// Each step's call of a Windows machine with `steps`.
fn calls(steps: &str, templates: &[(&str, &str)]) -> Vec<Option<String>> {
    let m = format!("os: windows-server@2022, ports: {{ web: 80 }}, app: {{ steps: {steps} }}");
    testing::rendered(&m, Target::Vagrant, templates, call)
}

/// The one call of a Windows machine with `step`.
fn one(step: &str) -> String {
    calls(&format!("[{step}]"), &[]).remove(0).expect("a call")
}

#[test]
fn features_files_and_copies() {
    assert_eq!(
        one("{ packages: [Web-Server, Web-Asp-Net45] }"),
        "Install-IsoloomFeature -Name 'Web-Server', 'Web-Asp-Net45'\n"
    );
    assert_eq!(
        one("{ copy: { from: site/, to: 'C:\\inetpub\\wwwroot' } }"),
        "Copy-IsoloomItem -From 'site/' -To 'C:\\inetpub\\wwwroot'\n"
    );
    assert_eq!(
        one("{ file: { path: 'C:\\data', state: directory } }"),
        "Install-IsoloomFile -Path 'C:\\data' -State directory\n"
    );
    let sha = "0".repeat(64);
    assert_eq!(
        one(&format!(
            "{{ file: {{ path: 'C:\\tool.zip', url: \"https://example.org/tool.zip\", sha256: {sha} }} }}"
        )),
        format!("Install-IsoloomFile -Path 'C:\\tool.zip' -State download -Url 'https://example.org/tool.zip' -Sha256 '{sha}'\n")
    );
}

#[test]
fn a_template_goes_in_filled_and_encoded() {
    let found = calls(
        "[{ template: { from: web.config, to: 'C:\\inetpub\\web.config' } }]",
        &[("web.config", "<x>{{ machine.name }}</x>")],
    );
    let encoded = isoloom_spec::shell::base64(b"<x>m</x>");
    assert_eq!(
        found[0].as_deref(),
        Some(format!("Install-IsoloomTemplate -To 'C:\\inetpub\\web.config' -Base64 '{encoded}'\n").as_str())
    );
}

#[test]
fn accounts_keep_their_password_plain_and_their_groups() {
    assert_eq!(
        one("{ user: { name: Alice, password: \"it's\", group: Users, groups: [Administrators] } }"),
        "Add-IsoloomUser -Name 'Alice' -Phrase 'it''s' -Groups 'Users', 'Administrators'\n"
    );
    assert_eq!(
        one("{ group: { name: Remote Desktop Users } }"),
        "Add-IsoloomGroup -Name 'Remote Desktop Users'\n"
    );
}

#[test]
fn services_commands_and_the_firewall() {
    assert_eq!(one("{ service: W3SVC }"), "Register-IsoloomService -Name 'W3SVC' -Enabled $true\n");
    assert_eq!(
        one("{ service: { name: api, run: ['C:\\api\\api.exe', --port, '8000'], cwd: 'C:\\api' } }"),
        "Register-IsoloomTask -Name 'api' -Run 'C:\\api\\api.exe', '--port', '8000' -Directory 'C:\\api'\nRegister-IsoloomTaskState -Name 'api' -Enabled $true\n"
    );
    assert_eq!(
        one("{ command: 'Enable-WindowsOptionalFeature -Online -FeatureName X', creates: 'C:\\done' }"),
        "Invoke-IsoloomCommand -Run 'Enable-WindowsOptionalFeature -Online -FeatureName X' -Creates 'C:\\done'\n"
    );
    assert_eq!(
        one("{ command: [net, user, x, /add], unless: [net, user, x] }"),
        "Invoke-IsoloomCommand -Run '& ''net'' ''user'' ''x'' ''/add''' -Unless '& ''net'' ''user'' ''x'''\n"
    );
    assert_eq!(
        one("{ firewall: { allow: [web, 53/udp] } }"),
        "Enable-IsoloomFirewallPort -Port 80 -Protocol TCP\nEnable-IsoloomFirewallPort -Port 53 -Protocol UDP\n"
    );
}

#[test]
fn a_vm_step_runs_the_library_then_its_calls_from_the_project_folder() {
    let env = testing::machine("os: windows-server@2022, app: { steps: [{ packages: [Web-Server] }] }", Target::Vagrant, &[]);
    let c = Configured::of(&env, "m").unwrap();
    let resources: Vec<Resource> = c.recipe().resources().cloned().collect();
    let s = script(&c, &resources, true);
    assert!(s.starts_with("$ErrorActionPreference = 'Stop'\n[Net.ServicePointManager]"), "{s}");
    assert!(!s.contains("\n# "), "the library goes in without its comments: {s}");
    assert!(
        s.ends_with("Set-Location -LiteralPath 'C:\\isoloom'\nInstall-IsoloomFeature -Name 'Web-Server'\n"),
        "{s}"
    );
    assert!(library_file().contents.ends_with(LIBRARY));
}
