//! A Windows machine set up at its first boot by cloudbase-init (Proxmox, ESXi): one
//! PowerShell script from its user data (`#ps1_sysnative`): the `isoloom` administrator with the
//! generated password, the machine's name (a restart, after which cloudbase-init runs the
//! script again), the project's files, its services' firewall openings, the plan's steps (their
//! Windows renderings), and last WinRM over HTTP for `isoloom`. So the controller's inventory
//! reaches it the way it reaches a cloud's Windows machines, and the controller waits for its
//! ready marker over WinRM before running the playbooks (`controller::windows_waits`).

use crate::hcl_builder::{Expression, Template, call, for_list, reference, string};
use isoloom_plan::{Environment, MachinePlan, Step, Vm};
use isoloom_render::steps;

use super::windows;

/// Where the set-up records that it is done (a later boot runs nothing).
const DONE: &str = "C:\\ProgramData\\isoloom\\done";

/// What a Windows machine set up this way can't have on `platform` (Proxmox, ESXi): what no
/// Windows VM set up at its own boot has, and inputs. Each reason after "machine `<name>`: ".
pub(crate) fn refusal(env: &Environment, platform: &str) -> Option<String> {
    env.vms().filter(|vm| vm.plan.is_windows()).find_map(|Vm { name, plan: mp, .. }| {
        isoloom_render::windows::set_up_at_boot_refusal(env, name)
            .or_else(|| (!mp.inputs.is_empty()).then(|| format!("machine `{name}`: {platform} delivers no inputs to a Windows machine")))
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
    if mp.lifecycle.iter().any(|s| matches!(s, Step::Run { .. }) || copies(s)) {
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

/// Whether a step copies from the project (a recipe's `copy`).
fn copies(step: &Step) -> bool {
    matches!(step, Step::Configure { resources, .. } if resources.iter().any(|r| matches!(r, isoloom_plan::configure::Resource::Copy(_))))
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
    Template::text(format!("# The project, at {dir}.\nfunction Write-IsoloomFile($path, $b64) {{ $f = Join-Path '{dir}' $path; New-Item -ItemType Directory -Force (Split-Path $f) | Out-Null; [IO.File]::WriteAllBytes($f, [Convert]::FromBase64String($b64)) }}\n", dir = isoloom_spec::windows::ISOLOOM_DIR))
        .with_expr(call("join", [string("\n"), for_list("f", reference("local.project_paths"), line, None)]))
        .with_text("\n")
}

/// WinRM over HTTP with Basic authentication for local administrators, its port open: last, so
/// a machine that answers is set up.
const WINRM: &str = "# WinRM for the controller, last: a machine that answers is set up.\nNew-ItemProperty -Path HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System -Name LocalAccountTokenFilterPolicy -Value 1 -PropertyType DWord -Force | Out-Null\nGet-NetConnectionProfile | Set-NetConnectionProfile -NetworkCategory Private -ErrorAction SilentlyContinue\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nif (-not (Get-NetFirewallRule -DisplayName 'isoloom winrm' -ErrorAction SilentlyContinue)) { New-NetFirewallRule -DisplayName 'isoloom winrm' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null }";
