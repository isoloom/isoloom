//! A Windows machine on Proxmox. Proxmox has no Windows cloud image: each Windows OS a spec uses
//! is a template you make once (installed from Microsoft's ISO with the VirtIO drivers and
//! cloudbase-init, UEFI on a Q35 machine, its system disk on `virtio0`, then generalized with
//! sysprep), named by its VM id in a variable (`template_windows_server_2022`). Each machine is
//! a full clone of it, with a virtual TPM when it gets one.
//!
//! Its set-up is one PowerShell script that cloudbase-init runs at the first boot from the
//! configuration drive (`#ps1_sysnative`): the `isoloom` administrator with the generated
//! password, the machine's name (a restart, after which cloudbase-init runs the script again),
//! the project's files, its services' firewall openings, the plan's steps (their Windows
//! renderings), and last WinRM over HTTP for `isoloom`. So the controller's inventory reaches
//! it the way it reaches a cloud's Windows machines, and the controller waits for its ready
//! marker over WinRM before running the playbooks.

use isoloom_plan::lifecycle::has_extension;
use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::{Block, Expression, Template, call, for_list, reference, string};
use isoloom_render::steps;
use isoloom_spec::images::is_windows;

use super::machine::Boot;
use crate::vm::cloud::WINDOWS_PASSWORD;
use crate::vm::module::Variable;
use crate::vm::{os, res, vms};

/// Where the set-up records that it is done (a later boot runs nothing).
const DONE: &str = "C:\\ProgramData\\isoloom\\done";

/// The variable holding the VM id of `os`'s template.
pub fn template_variable(os: &str) -> String {
    format!("template_{}", res(os))
}

/// One variable per Windows OS the machines use: its template's VM id.
pub fn variables(env: &Environment) -> Vec<Block> {
    let mut oses: Vec<&str> = vms(env).filter_map(|(_, mp)| os(mp)).filter(|o| is_windows(o)).collect();
    oses.sort_unstable();
    oses.dedup();
    oses.into_iter()
        .map(|o| {
            Variable::new(&template_variable(o), "number")
                .describe(&format!(
                    "The VM id of a `{o}` template: cloudbase-init and the VirtIO drivers installed, UEFI on Q35, its disk on virtio0, sysprepped"
                ))
                .block()
        })
        .collect()
}

/// The OS type Proxmox tunes a Windows VM for.
fn ostype(os: &str) -> &'static str {
    match os {
        "windows-server-2008r2" => "win7",
        "windows-server-2012r2" => "win8",
        "windows-10" | "windows-server-2016" | "windows-server-2019" => "win10",
        _ => "win11",
    }
}

/// A Windows machine's VM boots a full clone of its OS's template.
pub fn boot(mp: &MachinePlan) -> Boot {
    let o = os(mp).unwrap_or_default();
    Boot::Template {
        variable: template_variable(o),
        ostype: ostype(o),
        disk: mp.size.disk_gb,
    }
}

/// What Proxmox (or ESXi, `platform`, which sets Windows up the same way) can't give a Windows
/// machine yet: each reason after "machine `<name>`: ".
pub fn refusal(env: &Environment, platform: &str) -> Option<String> {
    let spec = env.spec();
    vms(env).filter(|(_, mp)| mp.is_windows()).find_map(|(name, mp)| {
        let why = if name.len() > 15 {
            "Windows computer names are at most 15 characters"
        } else if spec.machines[name].vm.as_ref()?.provision.iter().any(|s| !has_extension(s, "ps1")) {
            "Windows provisioning steps are PowerShell scripts (.ps1); Ansible runs from the controller (`provision:`)"
        } else if mp.default_gateway.is_some() || mp.is_gateway {
            "a gateway machine's routes on Windows come later"
        } else if mp.addresses.keys().any(|n| !env.networks[n].internet) {
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
pub fn user_data(env: &Environment, name: &str) -> Expression {
    let mp = &env.machines[name];
    let host = name.to_uppercase();
    let mut t = Template::text(format!(
        "#ps1_sysnative\n# Isoloom: machine `{name}`, set up by cloudbase-init at its first boot.\n$ErrorActionPreference = 'Stop'\nif (Test-Path '{DONE}') {{ exit 0 }}\nNew-Item -ItemType Directory -Force C:\\ProgramData\\isoloom | Out-Null\n# The administrator the controller's playbooks run as, over WinRM.\n$password = ConvertTo-SecureString '"
    ));
    t.push_expr(reference(WINDOWS_PASSWORD));
    t.push_text(format!(
        "' -AsPlainText -Force\nif (Get-LocalUser -Name isoloom -ErrorAction SilentlyContinue) {{ Set-LocalUser -Name isoloom -Password $password -PasswordNeverExpires $true }} else {{ New-LocalUser -Name isoloom -Password $password -PasswordNeverExpires | Out-Null }}\nif (-not (Get-LocalGroupMember -SID S-1-5-32-544 | Where-Object {{ $_.Name -like '*\\isoloom' }})) {{ Add-LocalGroupMember -SID S-1-5-32-544 -Member isoloom }}\n# Its name takes a restart: cloudbase-init runs this script again after it (exit code 1003).\nif ($env:COMPUTERNAME -ne '{host}') {{ Rename-Computer -NewName '{name}' -Force; exit 1003 }}\n"
    ));
    if mp.lifecycle.iter().any(|s| matches!(s, Step::Run { .. })) {
        t.append(project_files());
    }
    let mut ps: Vec<String> = env.spec().machines[name].ports.iter().flat_map(crate::vm::windows::firewall).collect();
    ps.extend(
        mp.lifecycle
            .iter()
            .filter(|s| **s != Step::Project)
            .filter_map(|s| steps::windows(env, name, s))
            .map(|s| s.trim_end().to_string()),
    );
    ps.push(WINRM.to_string());
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

/// The controller's wait for each Windows machine's ready marker, through Ansible over WinRM
/// (the inventory's own connection), before the playbooks run.
pub fn waits(env: &Environment) -> Vec<String> {
    vms(env)
        .filter(|(_, mp)| mp.is_windows())
        .map(|(name, _)| {
            format!(
                "i=0; until /opt/ansible/bin/ansible -i /etc/isoloom/inventory.ini {name} -m raw -a 'if (-not (Test-Path C:\\isoloom\\ready)) {{ exit 1 }}' >/dev/null 2>&1; do i=$((i+1)); [ $i -gt 360 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 10; done"
            )
        })
        .collect()
}
