//! The retry helpers, driven by a POSIX `sh` (they run on the Linux controller) with a fake
//! `ansible-playbook` that prints what Ansible prints: `fatal:` lines and a PLAY RECAP.

#![allow(clippy::unwrap_used)] // test helpers: a failing test is the report.

use std::path::{Path, PathBuf};

use isoloom_spec::shell::quote;

use super::{ATTEMPTS, RETRY_DIR, TRANSIENT, helpers, script};

/// One run of the fake: what it prints and its exit code.
struct Run {
    out: String,
    code: i32,
}

/// A host's line in the recap: its name, unreachable count, failed count.
type Host<'a> = (&'a str, u32, u32);

/// A recap line per host.
fn recap(hosts: &[Host]) -> String {
    let mut out = String::from("\nPLAY RECAP *********************************************************************\n");
    for (h, u, f) in hosts {
        out.push_str(&format!(
            "{h:<27}: ok=3    changed=1    unreachable={u}    failed={f}    skipped=0    rescued=0    ignored=0\n"
        ));
    }
    out
}

/// A `fatal:` line for `host` with `msg`, as the default callback writes it.
fn fatal(host: &str, msg: &str) -> String {
    format!("fatal: [{host}]: FAILED! => {{\"changed\": false, \"msg\": \"{msg}\"}}\n")
}

fn run(lines: &[String], hosts: &[Host], code: i32) -> Run {
    Run {
        out: format!("PLAY [all] ***\n{}{}", lines.concat(), recap(hosts)),
        code,
    }
}

fn ok() -> Run {
    run(&[], &[("web", 0, 0), ("dc01", 0, 0)], 0)
}

fn unreachable() -> Run {
    let line = "fatal: [dc01]: UNREACHABLE! => {\"msg\": \"Failed to connect to the host via ssh\", \"unreachable\": true}\n";
    run(&[line.into()], &[("web", 0, 0), ("dc01", 1, 0)], 4)
}

fn task_failed(host: &str) -> Run {
    run(
        &[fatal(host, "no such user")],
        &[("web", 0, u32::from(host == "web")), ("dc01", 0, u32::from(host == "dc01"))],
        2,
    )
}

struct Outcome {
    code: i32,
    out: String,
    /// The fake's arguments, one line per attempt.
    calls: Vec<String>,
    /// The hosts written for the retries.
    limit: String,
    dir: PathBuf,
}

/// Runs `run_play site.yml fake -i inv` under `sh -e` with no pauses; the fake answers each
/// attempt with the next of `runs` (then succeeds).
fn play(runs: &[Run]) -> Outcome {
    let dir = std::env::temp_dir().join(format!("isoloom-playbooks-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cases = String::new();
    for (i, r) in runs.iter().enumerate() {
        cases.push_str(&format!("    {}) printf '%s' {}; return {};;\n", i + 1, quote(&r.out), r.code));
    }
    let body = format!(
        "set -e\nCALLS={calls}\n{helpers}fake() {{\n  echo \"$*\" >> \"$CALLS\"\n  case $(wc -l < \"$CALLS\" | tr -d ' ') in\n{cases}    *) return 0;;\n  esac\n}}\nrun_play site.yml fake -i inv\necho after\n",
        calls = dir.join("calls").display(),
        helpers = helpers(),
    );
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&body)
        .env("ISOLOOM_RETRY_PAUSE", "0")
        .env("ISOLOOM_RETRY_DIR", &dir)
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    Outcome {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        calls,
        limit: std::fs::read_to_string(dir.join("site.retry")).unwrap_or_default(),
        dir,
    }
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn cleanup(dir: &Path) {
    std::fs::remove_dir_all(dir).ok();
}

/// The fake's arguments on a retry.
fn retried(o: &Outcome) -> String {
    format!("-i inv site.yml --limit @{}/site.retry", o.dir.display())
}

#[test]
fn a_play_failing_on_unreachable_hosts_only_runs_again_on_them() {
    let o = play(&[unreachable(), ok()]);
    assert_eq!(o.code, 0, "{}", o.out);
    assert_eq!(o.calls, ["-i inv site.yml".to_string(), retried(&o)]);
    assert_eq!(o.limit, "dc01\n");
    assert!(
        o.out
            .contains("isoloom: site.yml: unreachable or disconnected: dc01; waiting 0 s, then attempt 2/3 on them"),
        "{}",
        o.out
    );
    assert!(o.out.trim_end().ends_with("after"), "{}", o.out);
    cleanup(&o.dir);
}

/// A message of each kind the list knows, as Ansible and pywinrm word it.
const TRANSIENT_MESSAGES: &[&str] = &[
    "winrm send_input failed; \\nstdout: \\nstderr ",
    "The pipe has been ended.",
    "WinRMTransportError: (u'http', u'Bad HTTP response returned from server. Code 500')",
    "WinRMOperationTimeoutError",
    "HTTPSConnectionPool(host='10.0.0.5', port=5986): Read timeout. (read timeout=30)",
    "kerberos: Bad HTTP response returned from server. Code 500",
    "ntlm: the server answered 500 while the host restarted",
    "('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))",
    "[Errno 104] Connection reset by peer",
    "[Errno 111] Connection refused",
    "ssh: connect to host 10.0.0.5 port 22: Connection timed out",
    "Failed to connect to the host via ssh: kex_exchange_identification",
    "Shared connection to 10.0.0.5 closed.",
    "write: Broken pipe",
];

#[test]
fn a_task_cut_by_a_dropped_connection_is_retried_whatever_the_message() {
    for msg in TRANSIENT_MESSAGES {
        let cut = run(&[fatal("dc01", msg)], &[("web", 0, 0), ("dc01", 0, 1)], 2);
        let o = play(&[cut, ok()]);
        assert_eq!((o.code, o.calls.len()), (0, 2), "{msg}: {}", o.out);
        assert_eq!(o.limit, "dc01\n", "{msg}");
        cleanup(&o.dir);
    }
}

#[test]
fn a_task_failure_is_never_retried() {
    let o = play(&[task_failed("dc01"), ok()]);
    assert_eq!((o.code, o.calls.len()), (2, 1), "set -e stops the setup on the play's failure: {}", o.out);
    assert!(o.out.contains("isoloom: site.yml: a task failed on dc01: not retried"), "{}", o.out);
    assert!(!o.out.contains("after"));
    cleanup(&o.dir);
}

#[test]
fn a_host_with_a_real_failure_beside_a_dropped_connection_is_not_retried() {
    // The same host: one task cut by WinRM (in a loop), another failing on its own.
    let both = run(
        &[fatal("dc01", "winrm send_input failed"), fatal("dc01", "no such user")],
        &[("web", 0, 0), ("dc01", 0, 1)],
        2,
    );
    let o = play(&[both, ok()]);
    assert_eq!((o.code, o.calls.len()), (2, 1), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn mixed_hosts_retry_only_the_disconnected_ones_and_still_fail() {
    let mixed = run(
        &[
            fatal("web", "no such user"),
            fatal("dc01", "The pipe has been ended."),
            "fatal: [dc02]: UNREACHABLE! => {}\n".into(),
        ],
        &[("web", 0, 1), ("dc01", 0, 1), ("dc02", 1, 0)],
        2,
    );
    let o = play(&[mixed, ok()]);
    assert_eq!(o.calls, ["-i inv site.yml".to_string(), retried(&o)], "{}", o.out);
    assert_eq!(o.limit, "dc01\ndc02\n");
    assert!(o.out.contains("a task failed on web: not retried"), "{}", o.out);
    // dc01 and dc02 got through on the retry, web still failed: the setup stops.
    assert_eq!(o.code, 2, "{}", o.out);
    assert!(!o.out.contains("after"));
    cleanup(&o.dir);
}

#[test]
fn an_ignored_failure_doesnt_count() {
    let ignored = run(
        &[fatal("dc01", "no such user"), "...ignoring\n".into(), fatal("dc01", "Connection reset by peer")],
        &[("web", 0, 0), ("dc01", 0, 1)],
        2,
    );
    let o = play(&[ignored, ok()]);
    assert_eq!((o.code, o.calls.len()), (0, 2), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn hosts_that_stay_disconnected_give_up_after_the_last_attempt() {
    let o = play(&[unreachable(), unreachable(), unreachable(), ok()]);
    assert_eq!(o.code, 4, "{}", o.out);
    assert_eq!(o.calls.len(), usize::try_from(ATTEMPTS).unwrap());
    assert!(o.out.contains("still unreachable or disconnected after 3 attempts: dc01"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn no_recap_means_no_retry() {
    let syntax = Run {
        out: "ERROR! the playbook: site.yml could not be found\n".into(),
        code: 1,
    };
    let o = play(&[syntax, ok()]);
    assert_eq!((o.code, o.calls.len()), (1, 1), "{}", o.out);
    assert!(o.out.contains("with no unreachable host or dropped connection: not retried"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn the_output_is_read_through_colors_and_delegation() {
    let colored = Run {
        out: "\u{1b}[0;31mfatal: [dc01 -> localhost]: FAILED! => {\"msg\": \"Connection refused\"}\u{1b}[0m\nPLAY RECAP ***\n\u{1b}[0;31mdc01\u{1b}[0m : \u{1b}[0;32mok=2\u{1b}[0m changed=0 unreachable=0 \u{1b}[0;31mfailed=1\u{1b}[0m skipped=0\n"
            .into(),
        code: 2,
    };
    let o = play(&[colored, ok()]);
    assert_eq!((o.code, o.calls.len()), (0, 2), "{}", o.out);
    assert_eq!(o.limit, "dc01\n");
    cleanup(&o.dir);
}

#[test]
fn the_transient_list_fits_in_its_single_quotes() {
    assert!(TRANSIENT.iter().all(|t| !t.contains('\'') && *t == t.to_lowercase()));
    assert!(helpers().contains(&format!("_isoloom_attempts={ATTEMPTS}\n")));
    assert!(helpers().contains(&format!("${{ISOLOOM_RETRY_DIR:-{RETRY_DIR}}}")));
}

#[test]
fn a_failed_download_is_tried_again() {
    let run = |body: &str| {
        let s = format!(
            "set -e\n{}n=0\nget() {{ n=$((n + 1)); {body}; }}\nretry_download get\necho \"calls=$n\"\n",
            helpers()
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&s)
            .env("ISOLOOM_RETRY_PAUSE", "0")
            .output()
            .unwrap();
        (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let (code, out) = run("[ $n -ge 2 ]");
    assert_eq!(code, Some(0), "{out}");
    assert!(out.ends_with("calls=2\n"), "{out}");
    let (code, _) = run("return 1");
    assert_eq!(code, Some(1));
}

#[test]
fn the_script_runs_each_playbook_through_run_play() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lab: 10 }\n    vm: { os: debian-12 }\nprovision:\n  - ansible: ansible/site.yml\n    vars: { greeting: hi there }\n",
    )
    .unwrap();
    let s = script(&spec, &["web".into()]);
    assert!(s.contains(&helpers()), "{s}");
    assert!(
        s.contains("cd /opt/isoloom/ansible\n[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\nrun_play 'site.yml' ansible-playbook -i /etc/isoloom/inventory.ini -e '{\"greeting\":\"hi there\"}' --limit 'web'\n"),
        "{s}"
    );
}
