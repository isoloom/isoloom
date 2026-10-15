#[cfg(unix)]
use super::{DELIVERED, KEPT_DIR, host_environment, wait};
use super::{place, receive, step};
use isoloom_plan::SecretFile;

fn file(name: &str, path: &str, uid: Option<u32>, mode: Option<u32>) -> SecretFile {
    SecretFile {
        source: isoloom_plan::ValueSource::Secret(name.to_string()),
        name: name.into(),
        path: path.into(),
        uid,
        gid: None,
        mode,
    }
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
/// `script` run by `sh` with the kept folder moved to a temporary one, `input` on its standard
/// input; what it did, and the folder.
fn run(script: &str, input: &str, tag: &str) -> (std::process::Output, std::path::PathBuf) {
    use std::io::Write as _;
    let dir = std::env::temp_dir().join(format!("isoloom-secrets-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let script = script.replace(KEPT_DIR, &dir.display().to_string());
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    (child.wait_with_output().unwrap(), dir)
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
fn b64(s: &str) -> String {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("printf '%s' \"$V\" | base64 | tr -d '\\n'")
        .env("V", s)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn delivery_keeps_each_value_as_sent_then_marks_it() {
    let names = vec!["db-password".to_string(), "api-key".to_string(), "empty".to_string()];
    let values = ["p@ss 'word'\n", "line1\nline2", ""];
    let input: String = values.iter().map(|v| b64(v) + "\n").collect();
    let (out, dir) = run(&receive(&names, &[]), &input, "receive");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for (n, v) in names.iter().zip(values) {
        assert_eq!(std::fs::read_to_string(dir.join(n)).unwrap(), v);
    }
    assert!(dir.join(".delivered").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(dir.join("api-key")).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn a_missing_value_fails_the_delivery_without_the_mark() {
    let names = vec!["a".to_string(), "b".to_string()];
    let (out, dir) = run(&receive(&names, &[]), &format!("{}\n", b64("x")), "missing");
    assert!(!out.status.success());
    assert!(!dir.join(".delivered").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_value_is_in_a_script() {
    let files = [file("db", "/etc/app/db", Some(1000), Some(0o400))];
    let sh = receive(&["db".to_string()], &files);
    assert!(sh.contains("install -D -o 1000 -g 0 -m 0400 /var/lib/isoloom/secrets/db '/etc/app/db'"), "{sh}");
    assert_eq!(
        place(&[file("x", "/run/secrets/x", None, None)]),
        "install -D -o 0 -g 0 -m 0400 /var/lib/isoloom/secrets/x '/run/secrets/x'\n"
    );
}

#[test]
fn the_step_waits_then_places_at_every_boot() {
    let sh = step("web", &[file("db", "/run/secrets/db", None, None)]);
    assert!(sh.starts_with("set -e\nn=0; until [ -f /var/lib/isoloom/secrets/.delivered ]"), "{sh}");
    assert!(sh.contains("systemctl enable isoloom-apply-secrets.service"), "{sh}");
    assert!(sh.contains("install -D -o 0 -g 0 -m 0400"), "{sh}");
    let check = std::process::Command::new("sh").arg("-n").arg("-c").arg(&sh).status().unwrap();
    assert!(check.success());
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn the_wait_passes_once_delivered() {
    let (out, dir) = run(&format!("mkdir -p {KEPT_DIR} && touch {DELIVERED} && {}", wait("web")), "", "wait");
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "no waiting line once delivered");
    let _ = std::fs::remove_dir_all(&dir);
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn a_docker_host_reads_each_kept_value_into_its_variable() {
    assert_eq!(host_environment(&[]), "");
    let names = vec!["db-password".to_string()];
    let script = format!(
        "mkdir -p {KEPT_DIR} && printf 'v\\n\\n' > {KEPT_DIR}/db-password && touch {DELIVERED}\n{}printf '%s|' \"$ISOLOOM_SECRET_DB_PASSWORD\"",
        host_environment(&names)
    );
    let (out, dir) = run(&script, "", "host");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "v\n\n|");
    let _ = std::fs::remove_dir_all(&dir);
}
