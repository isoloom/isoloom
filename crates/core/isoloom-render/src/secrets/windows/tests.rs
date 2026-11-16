use isoloom_plan::SecretFile;

use super::{BOOTSTRAP, KEPT_DIR, command_line, from_environment, input, path, place, place_first, powershell_args, receive, step, wait};

fn file(name: &str, path: &str) -> SecretFile {
    SecretFile {
        source: isoloom_plan::ValueSource::Secret(name.to_owned()),
        name: name.into(),
        path: path.into(),
        uid: None,
        gid: None,
        mode: None,
    }
}

/// Standard base64 decoded (the tests read back what was encoded).
fn decode(s: &str) -> Vec<u8> {
    isoloom_test_support::b64_decode(s).expect("standard base64")
}

#[test]
fn a_path_from_root_is_on_drive_c() {
    assert_eq!(path("/run/secrets/db"), "C:\\run\\secrets\\db");
    assert_eq!(path("D:\\app\\db"), "D:\\app\\db");
}

#[test]
fn delivery_locks_the_folder_then_keeps_each_value_then_marks_it() {
    let names = vec!["db-password".to_owned(), "it's".to_owned()];
    let ps = receive(&names, &[file("db-password", "/run/secrets/db-password")]);
    // The folder locked to SYSTEM and Administrators, each value read from the standard input
    // into its file (names in order, quoted), the mark, then the files placed.
    assert_eq!(
        ps.lines().collect::<Vec<_>>(),
        [
            "$ErrorActionPreference = 'Stop'",
            "function Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; $acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { 'ContainerInherit, ObjectInherit' } else { 'None' }; foreach ($sid in 'S-1-5-18', 'S-1-5-32-544') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), 'FullControl', $inherit, 'None', 'Allow'))) }; Set-Acl -Path $p -AclObject $acl }",
            "$k = 'C:\\ProgramData\\isoloom\\secrets'",
            "New-Item -ItemType Directory -Force $k | Out-Null",
            "Protect-IsoloomPath $k",
            "foreach ($n in @('db-password', 'it''s')) { $v = [Console]::In.ReadLine(); if ($null -eq $v) { throw \"no value came for $n\" }; [IO.File]::WriteAllBytes(\"$k\\$n.new\", [Convert]::FromBase64String($v.Trim())); Move-Item -Force \"$k\\$n.new\" \"$k\\$n\" }",
            "Remove-Variable v",
            "Set-Content -Path 'C:\\ProgramData\\isoloom\\secrets\\.delivered' -Value ''",
            "$ErrorActionPreference = 'Stop'",
            "function Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; $acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { 'ContainerInherit, ObjectInherit' } else { 'None' }; foreach ($sid in 'S-1-5-18', 'S-1-5-32-544') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), 'FullControl', $inherit, 'None', 'Allow'))) }; Set-Acl -Path $p -AclObject $acl }",
            "$k = 'C:\\ProgramData\\isoloom\\secrets'",
            "function Set-IsoloomSecret($name, $path) { New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null; if (-not (Test-Path $path)) { New-Item -ItemType File $path | Out-Null }; Protect-IsoloomPath $path; [IO.File]::WriteAllBytes($path, [IO.File]::ReadAllBytes((Join-Path $k $name))) }",
            "Set-IsoloomSecret 'db-password' 'C:\\run\\secrets\\db-password'"
        ]
    );
}

#[test]
fn a_file_is_locked_before_it_holds_the_value() {
    assert_eq!(
        place(&[file("db", "C:\\app\\db")]),
        "$ErrorActionPreference = 'Stop'\nfunction Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; $acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { 'ContainerInherit, ObjectInherit' } else { 'None' }; foreach ($sid in 'S-1-5-18', 'S-1-5-32-544') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), 'FullControl', $inherit, 'None', 'Allow'))) }; Set-Acl -Path $p -AclObject $acl }\n$k = 'C:\\ProgramData\\isoloom\\secrets'\nfunction Set-IsoloomSecret($name, $path) { New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null; if (-not (Test-Path $path)) { New-Item -ItemType File $path | Out-Null }; Protect-IsoloomPath $path; [IO.File]::WriteAllBytes($path, [IO.File]::ReadAllBytes((Join-Path $k $name))) }\nSet-IsoloomSecret 'db' 'C:\\app\\db'\n",
        "locked, then written"
    );
}

#[test]
fn the_step_waits_then_places_now_and_at_every_boot() {
    let ps = step("win", &[file("db", "/run/secrets/db")]);
    assert!(ps.starts_with(&wait("win")), "it waits first: {ps}");
    assert!(ps.ends_with(&format!("{}\n", place_first())), "then places them now: {ps}");
    insta::assert_snapshot!(ps);
}

#[test]
fn the_wait_names_the_machine_and_the_way_out() {
    assert_eq!(
        wait("win"),
        "$t = (Get-Date).AddSeconds(1800); $said = $false; while (-not (Test-Path 'C:\\ProgramData\\isoloom\\secrets\\.delivered')) { if (-not $said) { Write-Output 'win: waiting for its secrets, which the tool that runs it delivers'; $said = $true }; if ((Get-Date) -gt $t) { throw 'win: its secrets never came; start the environment with a tool that delivers them' }; Start-Sleep 5 }\n",
        "half an hour of five-second tries, naming the machine and the way out"
    );
    assert_eq!(
        place_first(),
        "& powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\\ProgramData\\isoloom\\apply-secrets.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
    );
}

#[test]
fn the_command_line_is_the_bootstrap_alone() {
    let args = powershell_args();
    let encoded = args.last().expect("the encoded command");
    let utf16: Vec<u16> = decode(encoded).chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    assert_eq!(String::from_utf16(&utf16).expect("UTF-16"), BOOTSTRAP);
    // Nothing a remote shell (cmd or PowerShell) would expand or split differently.
    let line = command_line();
    assert!(line.chars().all(|c| c.is_ascii_alphanumeric() || " -+/=".contains(c)), "{line}");
    assert_eq!(
        line,
        format!("powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}")
    );
}

#[test]
fn the_input_is_the_script_then_the_values() {
    let names = vec!["db".to_owned()];
    let files = [file("db", "/run/secrets/db")];
    let bytes = input(&names, &files, b"c2VjcmV0\n");
    let text = String::from_utf8(bytes).expect("text");
    let (script, values) = text.split_once('\n').expect("two parts");
    assert_eq!(String::from_utf8(decode(script)).expect("UTF-8"), receive(&names, &files));
    assert_eq!(values, "c2VjcmV0\n");
    assert!(!receive(&names, &files).contains("c2VjcmV0") && !receive(&names, &files).contains(KEPT_DIR.replace('\\', "/").as_str()));
}

#[test]
fn an_inputs_default_is_written_when_no_value_is_given_and_none_is_kept() {
    let ps = from_environment(
        &[("TOKEN".to_owned(), "T_B64".to_owned())],
        &[("TOKEN".to_owned(), "dev-token".to_owned())],
        &[],
    );
    let lines: Vec<&str> = ps.lines().collect();
    // The default in base64, as a given value travels; used after a kept value would have won.
    assert_eq!(lines[5], "$d = @{ 'TOKEN' = 'ZGV2LXRva2Vu' }");
    assert!(
        lines[6].contains("{ continue }; if (-not $v) { $v = $d[$e.Key] }; $b = [byte[]]::new(0);"),
        "{}",
        lines[6]
    );
}

#[test]
fn values_from_the_environment_are_kept_then_placed_without_waiting() {
    let ps = from_environment(
        &[("db".to_owned(), "PW_B64".to_owned()), ("it's".to_owned(), "X".to_owned())],
        &[],
        &[file("db", "C:\\db.txt")],
    );
    // An empty value keeps the kept one; else an empty value writes an empty file: `$b` is
    // initialised to an empty byte array directly, not through `else { [byte[]]@() }`, whose
    // empty array PowerShell unwraps to $null -- which made WriteAllBytes throw "Value cannot be
    // null" the first time an unset input was delivered. Nothing waits.
    assert_eq!(
        ps.lines().collect::<Vec<_>>(),
        [
            "$ErrorActionPreference = 'Stop'",
            "function Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; $acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { 'ContainerInherit, ObjectInherit' } else { 'None' }; foreach ($sid in 'S-1-5-18', 'S-1-5-32-544') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), 'FullControl', $inherit, 'None', 'Allow'))) }; Set-Acl -Path $p -AclObject $acl }",
            "$k = 'C:\\ProgramData\\isoloom\\secrets'",
            "New-Item -ItemType Directory -Force $k | Out-Null",
            "Protect-IsoloomPath $k",
            "foreach ($e in @{ 'db' = 'PW_B64'; 'it''s' = 'X' }.GetEnumerator()) { $v = [Environment]::GetEnvironmentVariable($e.Value); if (-not $v -and (Test-Path \"$k\\$($e.Key)\")) { continue }; $b = [byte[]]::new(0); if ($v) { $b = [Convert]::FromBase64String($v) }; [IO.File]::WriteAllBytes(\"$k\\$($e.Key).new\", $b); Move-Item -Force \"$k\\$($e.Key).new\" \"$k\\$($e.Key)\" }",
            "Set-Content -Path 'C:\\ProgramData\\isoloom\\secrets\\.delivered' -Value ''",
            "Set-Content -Path 'C:\\ProgramData\\isoloom\\apply-secrets.ps1' -Value '$ErrorActionPreference = ''Stop''",
            "function Protect-IsoloomPath($p) { $dir = Test-Path -PathType Container $p; $acl = if ($dir) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }; $acl.SetAccessRuleProtection($true, $false); $inherit = if ($dir) { ''ContainerInherit, ObjectInherit'' } else { ''None'' }; foreach ($sid in ''S-1-5-18'', ''S-1-5-32-544'') { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier $sid), ''FullControl'', $inherit, ''None'', ''Allow''))) }; Set-Acl -Path $p -AclObject $acl }",
            "$k = ''C:\\ProgramData\\isoloom\\secrets''",
            "function Set-IsoloomSecret($name, $path) { New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null; if (-not (Test-Path $path)) { New-Item -ItemType File $path | Out-Null }; Protect-IsoloomPath $path; [IO.File]::WriteAllBytes($path, [IO.File]::ReadAllBytes((Join-Path $k $name))) }",
            "Set-IsoloomSecret ''db'' ''C:\\db.txt''",
            "'",
            "$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\\ProgramData\\isoloom\\apply-secrets.ps1'; Register-ScheduledTask -TaskName 'isoloom-apply-secrets' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null",
            "& powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\\ProgramData\\isoloom\\apply-secrets.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
        ]
    );
}
