use isoloom_plan::SecretFile;

use super::{BOOTSTRAP, DELIVERED, KEPT_DIR, PLACE_SCRIPT, command_line, input, path, place, place_first, powershell_args, receive, step, wait};

fn file(name: &str, path: &str) -> SecretFile {
    SecretFile {
        name: name.into(),
        path: path.into(),
        uid: None,
        gid: None,
        mode: None,
    }
}

/// Standard base64 decoded (the tests read back what was encoded).
fn decode(s: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    for chunk in s.trim_end_matches('=').as_bytes().chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, c)| n | (u32::from(value(*c)) << (18 - 6 * i)));
        out.extend(n.to_be_bytes()[1..chunk.len()].iter());
    }
    out
}

#[test]
fn a_path_from_root_is_on_drive_c() {
    assert_eq!(path("/run/secrets/db"), "C:\\run\\secrets\\db");
    assert_eq!(path("D:\\app\\db"), "D:\\app\\db");
}

#[test]
fn delivery_locks_the_folder_then_keeps_each_value_then_marks_it() {
    let names = vec!["db-password".to_string(), "it's".to_string()];
    let ps = receive(&names, &[file("db-password", "/run/secrets/db-password")]);
    let lock = ps.find("Protect-IsoloomPath $k").expect("the folder is locked");
    let keep = ps.find("foreach ($n in @('db-password', 'it''s'))").expect("names in order, quoted");
    let mark = ps.find(&format!("Set-Content -Path '{DELIVERED}'")).expect("the mark");
    let placed = ps.find("Set-IsoloomSecret 'db-password' 'C:\\run\\secrets\\db-password'").expect("placed");
    assert!(lock < keep && keep < mark && mark < placed, "{ps}");
    assert!(ps.contains("[Console]::In.ReadLine()"), "values come on the standard input: {ps}");
    assert!(
        ps.contains("'S-1-5-18', 'S-1-5-32-544'") && ps.contains("SetAccessRuleProtection($true, $false)"),
        "{ps}"
    );
}

#[test]
fn a_file_is_locked_before_it_holds_the_value() {
    let ps = place(&[file("db", "C:\\app\\db")]);
    let lock = ps.find("Protect-IsoloomPath $path;").expect("locked");
    let write = ps.find("[IO.File]::WriteAllBytes($path").expect("written");
    assert!(lock < write, "{ps}");
    assert!(ps.ends_with("Set-IsoloomSecret 'db' 'C:\\app\\db'\n"), "{ps}");
}

#[test]
fn the_step_waits_then_places_now_and_at_every_boot() {
    let ps = step("win", &[file("db", "/run/secrets/db")]);
    let waits = ps.find(&format!("Test-Path '{DELIVERED}'")).expect("waits");
    let task = ps.find("Register-ScheduledTask -TaskName 'isoloom-apply-secrets'").expect("startup task");
    assert!(
        waits < task && ps.contains("New-ScheduledTaskTrigger -AtStartup") && ps.contains("-User 'SYSTEM'"),
        "{ps}"
    );
    assert!(ps.contains(&format!("-File '{PLACE_SCRIPT}'; if ($LASTEXITCODE)")), "{ps}");
    insta::assert_snapshot!(ps);
}

#[test]
fn the_wait_names_the_machine_and_the_way_out() {
    let ps = wait("win");
    assert!(ps.contains("win: waiting for `isoloom run`") && ps.contains("AddSeconds(1800)"), "{ps}");
    assert!(place_first().contains(PLACE_SCRIPT));
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
}

#[test]
fn the_input_is_the_script_then_the_values() {
    let names = vec!["db".to_string()];
    let files = [file("db", "/run/secrets/db")];
    let bytes = input(&names, &files, b"c2VjcmV0\n");
    let text = String::from_utf8(bytes).expect("text");
    let (script, values) = text.split_once('\n').expect("two parts");
    assert_eq!(String::from_utf8(decode(script)).expect("UTF-8"), receive(&names, &files));
    assert_eq!(values, "c2VjcmV0\n");
    assert!(!receive(&names, &files).contains("c2VjcmV0") && !receive(&names, &files).contains(KEPT_DIR.replace('\\', "/").as_str()));
}
