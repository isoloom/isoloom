//! Secrets on a Windows VM, in PowerShell. Delivery keeps each value under [`KEPT_DIR`], a
//! folder only SYSTEM and Administrators can open, and leaves the mark [`DELIVERED`] the set-up
//! waits for; each file is then written at its path with the same two readers and nobody else.
//!
//! The values come on the connection's standard input (WinRM's or OpenSSH's), after the
//! receiving script itself: the command line `isoloom run` starts is [`BOOTSTRAP`], the same
//! for every machine, which reads the script's first line and runs it. Nothing of a value is
//! in a command line, a generated file or the output.
//!
//! At every boot a startup task (`isoloom-apply-secrets`) writes the files again. Task
//! Scheduler starts the startup tasks in no fixed order, so the `before` and command scripts
//! run the same placing script first themselves: their process never starts without its files.

use isoloom_plan::SecretFile;
use isoloom_spec::model::is_drive_path;
use isoloom_spec::shell::base64;

use crate::powershell;

/// Where a Windows machine keeps the secrets delivered to it.
pub const KEPT_DIR: &str = "C:\\ProgramData\\isoloom\\secrets";
/// The mark delivery leaves once every secret is kept.
pub const DELIVERED: &str = "C:\\ProgramData\\isoloom\\secrets\\.delivered";
/// The script putting the files in place, run by the startup task and before the process.
pub const PLACE_SCRIPT: &str = "C:\\ProgramData\\isoloom\\apply-secrets.ps1";
/// The startup task running [`PLACE_SCRIPT`] at every boot.
pub const PLACE_TASK: &str = "isoloom-apply-secrets";

/// What `isoloom run` starts on the machine (as `powershell -EncodedCommand`): it reads the
/// first line of its standard input, the receiving script in base64, and runs it. The script
/// reads the values from the lines that follow.
pub const BOOTSTRAP: &str = "$s = [Console]::In.ReadLine(); Invoke-Expression ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s)))";

/// Limits a file or folder to SYSTEM (S-1-5-18) and Administrators (S-1-5-32-544), nothing
/// inherited: a new access list replaces whatever the path had.
const PROTECT: &str = "function Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; \
$acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; \
$acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { 'ContainerInherit, ObjectInherit' } else { 'None' }; \
foreach ($sid in 'S-1-5-18', 'S-1-5-32-544') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), 'FullControl', $inherit, 'None', 'Allow'))) }; \
Set-Acl -Path $p -AclObject $acl }";

/// The path of a secret file on Windows: a drive path as it is, a path from `/` on drive C:.
#[must_use]
pub fn path(p: &str) -> String {
    if is_drive_path(p) {
        p.to_owned()
    } else {
        format!("C:{}", p.replace('/', "\\"))
    }
}

/// The receiving script. After the line that holds it, its standard input holds one line per
/// secret of `names`, in that order, each value in base64; it keeps each one, marks the
/// delivery, then puts `files` in place (so a run again refreshes them).
#[must_use]
pub fn receive(names: &[String], files: &[SecretFile]) -> String {
    let list = names.iter().map(|n| powershell::quote(n)).collect::<Vec<_>>().join(", ");
    format!(
        "$ErrorActionPreference = 'Stop'\n{PROTECT}\n$k = {k}\nNew-Item -ItemType Directory -Force $k | Out-Null\nProtect-IsoloomPath $k\n\
         foreach ($n in @({list})) {{ $v = [Console]::In.ReadLine(); if ($null -eq $v) {{ throw \"no value came for $n\" }}; [IO.File]::WriteAllBytes(\"$k\\$n.new\", [Convert]::FromBase64String($v.Trim())); Move-Item -Force \"$k\\$n.new\" \"$k\\$n\" }}\n\
         Remove-Variable v\nSet-Content -Path {} -Value ''\n{}",
        powershell::quote(DELIVERED),
        place(files),
        k = powershell::quote(KEPT_DIR)
    )
}

/// Each kept secret written at its path, its folder made when missing. The file is created
/// empty and limited to SYSTEM and Administrators before the value is written into it, so it
/// never holds the value with its folder's (wider) access list.
#[must_use]
pub fn place(files: &[SecretFile]) -> String {
    let set: String = files
        .iter()
        .map(|f| format!("Set-IsoloomSecret {} {}\n", powershell::quote(&f.name), powershell::quote(&path(&f.path))))
        .collect::<Vec<_>>()
        .concat();
    format!(
        "$ErrorActionPreference = 'Stop'\n{PROTECT}\n$k = {}\n\
         function Set-IsoloomSecret($name, $path) {{ New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null; \
         if (-not (Test-Path $path)) {{ New-Item -ItemType File $path | Out-Null }}; Protect-IsoloomPath $path; \
         [IO.File]::WriteAllBytes($path, [IO.File]::ReadAllBytes((Join-Path $k $name))) }}\n{set}",
        powershell::quote(KEPT_DIR)
    )
}

/// Waits for the delivery mark, saying so once; fails after
/// [`DELIVERY_TIMEOUT`](super::DELIVERY_TIMEOUT) seconds with what to do.
#[must_use]
pub fn wait(who: &str) -> String {
    let waiting = powershell::quote(&format!("{who}: waiting for `isoloom run` to deliver its secrets"));
    let missing = powershell::quote(&format!(
        "{who}: its secrets never came; start the environment with `isoloom run`, which reads them from their stores and delivers them"
    ));
    format!(
        "$t = (Get-Date).AddSeconds({}); $said = $false; while (-not (Test-Path {})) {{ if (-not $said) {{ Write-Output {waiting}; $said = $true }}; if ((Get-Date) -gt $t) {{ throw {missing} }}; Start-Sleep 5 }}\n",
        super::DELIVERY_TIMEOUT,
        powershell::quote(DELIVERED)
    )
}

/// A Windows VM's `Secrets` step: the wait, then the placing script written, registered as a
/// startup task (as SYSTEM) and run now.
#[must_use]
pub fn step(machine: &str, files: &[SecretFile]) -> String {
    format!(
        "{}Set-Content -Path {script} -Value {body}\n\
         $a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File {PLACE_SCRIPT}'; \
         Register-ScheduledTask -TaskName '{PLACE_TASK}' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null\n\
         & powershell -NoProfile -ExecutionPolicy Bypass -File {script}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}\n",
        wait(machine),
        script = powershell::quote(PLACE_SCRIPT),
        body = powershell::quote(&place(files)),
    )
}

/// The line a `before` or command script starts with on a machine that gets secrets: the files
/// put in place first, whichever startup task Windows ran first.
#[must_use]
pub fn place_first() -> String {
    format!(
        "& powershell -NoProfile -ExecutionPolicy Bypass -File {}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
        powershell::quote(PLACE_SCRIPT)
    )
}

/// The arguments of `powershell.exe` running [`BOOTSTRAP`], encoded (UTF-16LE in base64) so
/// that no remote shell (cmd, PowerShell) reads anything into it.
#[must_use]
pub fn powershell_args() -> Vec<String> {
    let bytes: Vec<u8> = BOOTSTRAP.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut args: Vec<String> = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"]
        .map(String::from)
        .to_vec();
    args.push(base64(&bytes));
    args
}

/// The command line running [`BOOTSTRAP`] through a remote shell (OpenSSH's): `powershell`
/// and [`powershell_args`], none of which a shell changes.
#[must_use]
pub fn command_line() -> String {
    format!("powershell {}", powershell_args().join(" "))
}

/// What the deliverer writes on the connection's standard input: the receiving script in
/// base64 on the first line, then `values` (one base64 line per secret, in `names`' order).
#[must_use]
pub fn input(names: &[String], files: &[SecretFile], values: &[u8]) -> Vec<u8> {
    let mut out = base64(receive(names, files).as_bytes()).into_bytes();
    out.push(b'\n');
    out.extend_from_slice(values);
    out
}

#[cfg(test)]
mod tests;
