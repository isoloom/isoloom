//! A Windows machine set up at its first boot by cloudbase-init (Proxmox, ESXi): one
//! PowerShell script from its user data (`#ps1_sysnative`): the `isoloom` administrator with the
//! generated password, the machine's name (a restart, after which cloudbase-init runs the
//! script again), the project's files, its services' firewall openings, the plan's steps (their
//! Windows renderings), and last WinRM over HTTP for `isoloom`. So the controller's inventory
//! reaches it the way it reaches a cloud's Windows machines, and the controller waits for its
//! ready marker over WinRM before running the playbooks (`controller::windows_waits`).

use isoloom_plan::lifecycle::has_extension;
use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::{Expression, Template, call, for_list, reference, string};
use isoloom_render::steps;

use super::{vms, windows};

/// Where the set-up records that it is done (a later boot runs nothing).
const DONE: &str = "C:\\ProgramData\\isoloom\\done";

/// What a Windows machine set up this way can't have yet on `platform` (Proxmox, ESXi): each
/// reason after "machine `<name>`: ".
pub(crate) fn refusal(env: &Environment, platform: &str) -> Option<String> {
    let spec = env.spec();
    vms(env).filter(|(_, mp)| mp.is_windows()).find_map(|(name, mp)| {
        let why = if name.len() > 15 {
            "Windows computer names are at most 15 characters"
        } else if spec.machines.get(name)?.vm.as_ref()?.provision.iter().any(|s| !has_extension(s, "ps1")) {
            "Windows provisioning steps are PowerShell scripts (.ps1); Ansible runs from the controller (`provision:`)"
        } else if mp.default_gateway.is_some() || mp.is_gateway {
            "a gateway machine's routes on Windows come later"
        } else if mp.addresses.keys().any(|n| env.networks.get(n).is_some_and(|p| !p.internet)) {
            "`internet: false` on Windows comes later"
        } else if !mp.volumes.is_empty() {
            "volumes on Windows come later"
        } else if !mp.inputs.is_empty() {
            return Some(format!("machine `{name}`: inputs on a Windows machine on {platform} come later"));
        } else {
            return None;
        };
        Some(format!("machine `{name}`: {why}"))
    })
}

/// The cloudbase-init script of Windows machine `name`.
pub(crate) fn user_data(env: &Environment, (name, mp): (&str, &MachinePlan)) -> Expression {
    let host = name.to_uppercase();
    let mut t = Template::text(format!(
        "#ps1_sysnative\n# Isoloom: machine `{name}`, set up by cloudbase-init at its first boot.\n$ErrorActionPreference = 'Stop'\nif (Test-Path '{DONE}') {{ exit 0 }}\nNew-Item -ItemType Directory -Force C:\\ProgramData\\isoloom | Out-Null\n# The administrator the controller's playbooks run as, over WinRM.\n$password = ConvertTo-SecureString '"
    ));
    t.push_expr(reference(windows::PASSWORD));
    t.push_text(format!(
        "' -AsPlainText -Force\nif (Get-LocalUser -Name isoloom -ErrorAction SilentlyContinue) {{ Set-LocalUser -Name isoloom -Password $password -PasswordNeverExpires $true }} else {{ New-LocalUser -Name isoloom -Password $password -PasswordNeverExpires | Out-Null }}\nif (-not (Get-LocalGroupMember -SID S-1-5-32-544 | Where-Object {{ $_.Name -like '*\\isoloom' }})) {{ Add-LocalGroupMember -SID S-1-5-32-544 -Member isoloom }}\n# Its name takes a restart: cloudbase-init runs this script again after it (exit code 1003).\nif ($env:COMPUTERNAME -ne '{host}') {{ Rename-Computer -NewName '{name}' -Force; exit 1003 }}\n"
    ));
    if mp.lifecycle.iter().any(|s| matches!(s, Step::Run { .. })) {
        t.append(project_files());
    }
    let ports = env.spec().machines.get(name).map(|m| m.ports.as_slice()).unwrap_or_default();
    let mut ps: Vec<String> = ports.iter().flat_map(windows::firewall).collect();
    ps.extend(
        mp.lifecycle
            .iter()
            .filter(|s| **s != Step::Project)
            .filter_map(|s| steps::windows(env, name, s))
            .map(|s| s.trim_end().to_owned()),
    );
    ps.push(WINRM.to_owned());
    ps.push(format!("New-Item -ItemType File -Force '{DONE}' | Out-Null"));
    t.push_text(ps.join("\n") + "\n");
    t.into_expression()
}

/// The project's files, each written under `C:\isoloom` from its content (cloudbase-init has no
/// `write_files`): the paths `.isoloomignore` keeps, read when Terraform runs.
fn project_files() -> Template {
    let f = || reference("f");
    let path = call("replace", [f(), string("'"), string("''")]);
    let content = call(
        "filebase64",
        [Template::new()
            .with_expr(reference("local.root"))
            .with_text("/")
            .with_expr(f())
            .into_expression()],
    );
    let line = Template::text("Write-IsoloomFile '")
        .with_expr(path)
        .with_text("' '")
        .with_expr(content)
        .with_text("'")
        .into_expression();
    Template::text("# The project, at C:\\isoloom.\nfunction Write-IsoloomFile($path, $b64) { $f = Join-Path 'C:\\isoloom' $path; New-Item -ItemType Directory -Force (Split-Path $f) | Out-Null; [IO.File]::WriteAllBytes($f, [Convert]::FromBase64String($b64)) }\n")
        .with_expr(call("join", [string("\n"), for_list("f", reference("local.project_paths"), line, None)]))
        .with_text("\n")
}

/// WinRM over HTTP with Basic authentication for local administrators, its port open: last, so
/// a machine that answers is set up.
const WINRM: &str = "# WinRM for the controller, last: a machine that answers is set up.\nNew-ItemProperty -Path HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System -Name LocalAccountTokenFilterPolicy -Value 1 -PropertyType DWord -Force | Out-Null\nGet-NetConnectionProfile | Set-NetConnectionProfile -NetworkCategory Private -ErrorAction SilentlyContinue\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nif (-not (Get-NetFirewallRule -DisplayName 'isoloom winrm' -ErrorAction SilentlyContinue)) { New-NetFirewallRule -DisplayName 'isoloom winrm' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null }";
