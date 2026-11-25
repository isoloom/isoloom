#[cfg(unix)]
use super::{DELIVERED, KEPT_DIR, from_environment, host_environment};
use super::{compose_variable, of_containers, place, receive, step, wait};
use crate::testing;
use isoloom_plan::SecretFile;
use isoloom_spec::model::Target;

/// Two secrets and an input's file: `web` (a container) gets `db-password` and the input, `vm`
/// (a Windows VM) gets `api-key`.
const LAB: &str = r"
version: 1
name: s
inputs: [DEMO_SECRET]
secrets:
  api-key: { env: API_KEY }
  db-password: { env: DB_PASSWORD }
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    app: { image: nginx }
    files: [{ path: /run/secrets/db, secret: db-password }, { path: /secret, input: DEMO_SECRET }]
  vm:
    networks: { lan: 11 }
    os: windows-server@2022
    files: [{ path: 'C:\key', secret: api-key }]
";

/// [`LAB`]'s containers alone, and their secrets: what a Docker host runs.
fn containers_only() -> String {
    let containers = LAB.split("  vm:").next().unwrap_or(LAB);
    containers.replace("  api-key: { env: API_KEY }\n", "")
}

fn file(name: &str, path: &str, uid: Option<u32>, mode: Option<u32>) -> SecretFile {
    SecretFile {
        source: isoloom_plan::ValueSource::Secret(name.to_owned()),
        name: name.into(),
        path: path.into(),
        uid,
        gid: None,
        mode,
    }
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
/// `script` run by `sh` with the kept folder moved into a temporary one, `input` on its standard
/// input; what it did, the kept folder (which the script makes), and the temporary folder that
/// holds it, removed when it goes.
fn run(script: &str, input: &str, tag: &str) -> (std::process::Output, std::path::PathBuf, tempfile::TempDir) {
    let scratch = tempfile::Builder::new().prefix(&format!("isoloom-secrets-{tag}-")).tempdir().unwrap();
    let dir = scratch.path().join("kept");
    let script = script.replace(KEPT_DIR, &dir.display().to_string());
    let out = isoloom_test_support::output_with_input(std::process::Command::new("sh").arg("-c").arg(script), input.as_bytes()).unwrap();
    (out, dir, scratch)
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
fn b64(s: &str) -> String {
    isoloom_spec::shell::base64(s.as_bytes())
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn delivery_keeps_each_value_as_sent_then_marks_it() {
    let names = vec!["db-password".to_owned(), "api-key".to_owned(), "empty".to_owned()];
    let values = ["p@ss 'word'\n", "line1\nline2", ""];
    let input: String = values.iter().map(|v| b64(v) + "\n").collect();
    let (out, dir, _scratch) = run(&receive(&names, &[]), &input, "receive");
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
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn a_missing_value_fails_the_delivery_without_the_mark() {
    let names = vec!["a".to_owned(), "b".to_owned()];
    let (out, dir, _scratch) = run(&receive(&names, &[]), &format!("{}\n", b64("x")), "missing");
    assert!(!out.status.success());
    assert!(!dir.join(".delivered").exists());
}

#[test]
fn no_value_is_in_a_script() {
    let files = [file("db", "/etc/app/db", Some(1000), Some(0o400))];
    let sh = receive(&["db".to_owned()], &files);
    assert_eq!(
        sh,
        "set -e\numask 077\nmkdir -p /var/lib/isoloom/secrets\nchmod 700 /var/lib/isoloom/secrets\nIFS= read -r v || [ -n \"$v\" ]; printf '%s\\n' \"$v\" | base64 -d > /var/lib/isoloom/secrets/db.new; mv -f /var/lib/isoloom/secrets/db.new /var/lib/isoloom/secrets/db\nunset v\ntouch /var/lib/isoloom/secrets/.delivered\ninstall -D -o 1000 -g 0 -m 0400 /var/lib/isoloom/secrets/db '/etc/app/db'\n",
        "the values come on the standard input, never in the script"
    );
    assert_eq!(
        place(&[file("x", "/run/secrets/x", None, None)]),
        "install -D -o 0 -g 0 -m 0400 /var/lib/isoloom/secrets/x '/run/secrets/x'\n"
    );
}

#[test]
fn the_step_waits_then_places_at_every_boot() {
    let files = [file("db", "/run/secrets/db", None, None)];
    let sh = step("web", &files);
    assert_eq!(
        sh,
        "set -e\ni=0; until [ -f /var/lib/isoloom/secrets/.delivered ]; do [ \"$i\" -gt 0 ] || echo 'web: waiting for its secrets, which the tool that runs it delivers'; i=$((i+5)); if [ \"$i\" -gt 1800 ]; then echo 'web: its secrets never came; start the environment with a tool that delivers them' >&2; exit 1; fi; sleep 5; done\nmkdir -p /etc/isoloom\ncat > /etc/isoloom/apply-secrets.sh <<'APPLY-SECRETS'\n#!/bin/sh\n# The machine's secrets, put in place\ninstall -D -o 0 -g 0 -m 0400 /var/lib/isoloom/secrets/db '/run/secrets/db'\nexit 0\nAPPLY-SECRETS\nchmod +x /etc/isoloom/apply-secrets.sh\nif command -v systemctl >/dev/null; then\ncat > /etc/systemd/system/isoloom-apply-secrets.service <<'UNIT'\n[Unit]\nDescription=The machine's secrets, put in place\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=-/etc/isoloom/apply-secrets.sh\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-apply-secrets.service\nsystemctl restart isoloom-apply-secrets.service\nelse\nmkdir -p /etc/local.d && ln -sf /etc/isoloom/apply-secrets.sh /etc/local.d/isoloom-apply-secrets.start\nrc-update add local default >/dev/null 2>&1 || true\n/etc/isoloom/apply-secrets.sh || true\nfi\n",
        "the wait, then the placing kept as a boot service"
    );
    if let Some(mut posix) = crate::test_support::posix_sh() {
        assert!(posix.arg("-n").arg("-c").arg(&sh).status().unwrap().success(), "{sh}");
    }
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn the_wait_passes_once_delivered() {
    let (out, _, _scratch) = run(&format!("mkdir -p {KEPT_DIR} && touch {DELIVERED} && {}", wait("web")), "", "wait");
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "no waiting line once delivered");
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn a_docker_host_reads_each_kept_value_into_its_variable() {
    let env = testing::env(&containers_only(), Target::CloudDocker);
    assert_eq!(host_environment(&env, &[]), "");
    let names = vec!["db-password".to_owned()];
    let script = format!(
        "mkdir -p {KEPT_DIR} && printf 'v\\n\\n' > {KEPT_DIR}/db-password && touch {DELIVERED}\n{}printf '%s|' \"$ISOLOOM_SECRET_DB_PASSWORD\"",
        host_environment(&env, &names)
    );
    let (out, _, _scratch) = run(&script, "", "host");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "v\n\n|");
}

#[test]
fn the_wait_gives_up_after_half_an_hour_of_five_second_tries() {
    let sh = wait("web");
    assert_eq!(
        sh,
        "i=0; until [ -f /var/lib/isoloom/secrets/.delivered ]; do [ \"$i\" -gt 0 ] || echo 'web: waiting for its secrets, which the tool that runs it delivers'; i=$((i+5)); if [ \"$i\" -gt 1800 ]; then echo 'web: its secrets never came; start the environment with a tool that delivers them' >&2; exit 1; fi; sleep 5; done\n"
    );
    assert!(!sh.contains('`'), "no backquote, which shellcheck reads as an expansion left unexpanded: {sh}");
}

#[test]
fn a_docker_host_gets_the_secrets_of_its_containers_in_the_spec_s_order() {
    let env = testing::env(&containers_only(), Target::CloudDocker);
    assert_eq!(of_containers(&env), ["db-password", "input-demo-secret"]);
    assert_eq!(compose_variable(&env, "db-password"), "ISOLOOM_SECRET_DB_PASSWORD");
    assert_eq!(
        compose_variable(&env, "input-demo-secret"),
        "DEMO_SECRET",
        "an input's file reads the input itself"
    );
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn values_from_the_environment_are_kept_and_an_empty_one_keeps_the_kept_value() {
    let script = from_environment(&[("db".to_owned(), "PW_B64".to_owned()), ("api".to_owned(), "API_B64".to_owned())], &[], &[]);
    // Only what keeps the values: the placing service needs a real machine's `/etc`.
    let (keeping, placing) = script.split_once(&format!("touch {DELIVERED}\n")).expect("the mark");
    assert_eq!(
        placing,
        "mkdir -p /etc/isoloom\ncat > /etc/isoloom/apply-secrets.sh <<'APPLY-SECRETS'\n#!/bin/sh\n# The machine's secrets, put in place\nexit 0\nAPPLY-SECRETS\nchmod +x /etc/isoloom/apply-secrets.sh\nif command -v systemctl >/dev/null; then\ncat > /etc/systemd/system/isoloom-apply-secrets.service <<'UNIT'\n[Unit]\nDescription=The machine's secrets, put in place\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=-/etc/isoloom/apply-secrets.sh\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-apply-secrets.service\nsystemctl restart isoloom-apply-secrets.service\nelse\nmkdir -p /etc/local.d && ln -sf /etc/isoloom/apply-secrets.sh /etc/local.d/isoloom-apply-secrets.start\nrc-update add local default >/dev/null 2>&1 || true\n/etc/isoloom/apply-secrets.sh || true\nfi\n",
        "placed now and at every boot, without waiting"
    );
    let first = format!("export PW_B64={}; {keeping}touch {DELIVERED}\n", b64("p@ss 'word'\n"));
    let (fresh, fresh_dir, _fresh) = run(&first, "", "env-first");
    assert!(fresh.status.success(), "{}", String::from_utf8_lossy(&fresh.stderr));
    assert_eq!(std::fs::read_to_string(fresh_dir.join("db")).unwrap(), "p@ss 'word'\n");
    let unset = std::fs::read_to_string(fresh_dir.join("api")).unwrap();
    assert_eq!(unset, "", "an unset variable writes an empty value the first time");
    assert!(fresh_dir.join(".delivered").exists());
    // Again with no value: the kept one stays (`run` starts from an empty folder, so seed it).
    let again = format!("mkdir -p {KEPT_DIR} && printf kept > {KEPT_DIR}/db && {keeping}");
    let (kept, kept_dir, _kept) = run(&again, "", "env-again");
    assert!(kept.status.success(), "{}", String::from_utf8_lossy(&kept.stderr));
    assert_eq!(std::fs::read_to_string(kept_dir.join("db")).unwrap(), "kept");
}

// The script runs on a Linux machine: run it where `sh` and Unix paths are the host's.
#[cfg(unix)]
#[test]
fn an_inputs_default_is_kept_when_no_value_is_given_the_first_time_only() {
    let values = [("TOKEN".to_owned(), "TOKEN_B64".to_owned()), ("db".to_owned(), "PW_B64".to_owned())];
    let script = from_environment(&values, &[("TOKEN".to_owned(), "dev 'token' $HOME".to_owned())], &[]);
    let (keeping, _) = script.split_once(&format!("touch {DELIVERED}\n")).expect("the mark");
    // No value, nothing kept: the default, written as it is; the value without one stays empty.
    let (first, first_dir, _first) = run(keeping, "", "default-first");
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    assert_eq!(std::fs::read_to_string(first_dir.join("TOKEN")).unwrap(), "dev 'token' $HOME");
    assert_eq!(std::fs::read_to_string(first_dir.join("db")).unwrap(), "");
    // A value given wins over the default.
    let given = format!("export TOKEN_B64={}; {keeping}", b64("launched"));
    let (g, g_dir, _g) = run(&given, "", "default-given");
    assert!(g.status.success(), "{}", String::from_utf8_lossy(&g.stderr));
    assert_eq!(std::fs::read_to_string(g_dir.join("TOKEN")).unwrap(), "launched");
    // No value again: the kept value stays, the default doesn't replace it.
    let again = format!("mkdir -p {KEPT_DIR} && printf launched > {KEPT_DIR}/TOKEN && {keeping}");
    let (k, k_dir, _k) = run(&again, "", "default-again");
    assert!(k.status.success(), "{}", String::from_utf8_lossy(&k.stderr));
    assert_eq!(std::fs::read_to_string(k_dir.join("TOKEN")).unwrap(), "launched");
}

#[test]
fn the_secrets_are_kept_in_isoloom_s_own_linux_folder() {
    assert!(
        super::KEPT_DIR.starts_with(&format!("{}/", isoloom_spec::linux::STATE_DIR)),
        "{}",
        super::KEPT_DIR
    );
}
