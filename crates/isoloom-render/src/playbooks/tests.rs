//! The retry helpers, driven by a POSIX `sh` (they run on the Linux controller) with a fake
//! `ansible-playbook` that prints what Ansible prints: `fatal:` lines and a PLAY RECAP. The
//! cases of the dev branch's retry (#125) are here too: unreachable hosts alone run again on
//! their own, a failed task beside an unreachable host isn't retried, nor is a playbook that
//! doesn't parse, and the runs stop after four.

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

/// An `UNREACHABLE!` line for `host`.
fn gone(host: &str) -> String {
    format!("fatal: [{host}]: UNREACHABLE! => {{\"msg\": \"ntlm: the specified credentials were rejected by the server\", \"unreachable\": true}}\n")
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
    run(&[gone("dc01")], &[("web", 0, 0), ("dc01", 1, 0)], 4)
}

struct Outcome {
    code: i32,
    out: String,
    /// The fake's arguments, one line per run, the folder written `$DIR`.
    calls: Vec<String>,
    /// The hosts in the limit file, as each retry saw them.
    limits: Vec<String>,
    dir: PathBuf,
}

/// Runs `run_play fake -i inv site.yml` under `sh -e` with no pauses; the fake answers each run
/// with the next of `runs` (then succeeds).
fn play(runs: &[Run]) -> Outcome {
    let dir = std::env::temp_dir().join(format!("isoloom-playbooks-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cases = String::new();
    for (i, r) in runs.iter().enumerate() {
        cases.push_str(&format!("    {}) printf '%s' {}; return {};;\n", i + 1, quote(&r.out), r.code));
    }
    let body = format!(
        "set -e\nD={d}\n{helpers}fake() {{\n  echo \"$*\" >> \"$D/calls\"\n  case \"$*\" in *--limit*) echo $(cat \"$D/site.retry\") >> \"$D/limits\";; esac\n  case $(wc -l < \"$D/calls\" | tr -d ' ') in\n{cases}    *) return 0;;\n  esac\n}}\nrun_play fake -i inv site.yml\necho after\n",
        d = dir.display(),
        helpers = helpers(),
    );
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&body)
        .env("ISOLOOM_RETRY_PAUSE", "0")
        .env("ANSIBLE_RETRY_FILES_SAVE_PATH", &dir)
        .output()
        .unwrap();
    let lines = |f: &str| -> Vec<String> {
        std::fs::read_to_string(dir.join(f))
            .unwrap_or_default()
            .lines()
            .map(|l| l.replace(&dir.display().to_string(), "$DIR"))
            .collect()
    };
    Outcome {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        calls: lines("calls"),
        limits: lines("limits"),
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

const FIRST: &str = "-i inv site.yml";
const AGAIN: &str = "-i inv site.yml --limit @$DIR/site.retry";

#[test]
fn unreachable_hosts_alone_are_run_again_on_their_own_after_a_pause() {
    // dc01 and srv02 drop WinRM after a reboot; the next run reaches dc01, the third srv02.
    let both = run(&[gone("dc01"), gone("srv02")], &[("web", 0, 0), ("dc01", 1, 0), ("srv02", 1, 0)], 4);
    let one = run(&[gone("srv02")], &[("dc01", 0, 0), ("srv02", 1, 0)], 4);
    let o = play(&[both, one, ok()]);
    assert_eq!(o.code, 0, "{}", o.out);
    assert_eq!(o.calls, [FIRST, AGAIN, AGAIN]);
    assert_eq!(o.limits, ["dc01 srv02", "srv02"]);
    assert!(
        o.out
            .contains("isoloom: site.yml: unreachable: dc01 srv02; waiting 0 s, then running it again on them (2/4)"),
        "{}",
        o.out
    );
    assert!(
        o.out
            .contains("isoloom: site.yml: unreachable: srv02; waiting 0 s, then running it again on them (3/4)"),
        "{}",
        o.out
    );
    assert!(o.out.trim_end().ends_with("after"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn hosts_that_stay_unreachable_stop_the_setup_after_four_runs() {
    let o = play(&[unreachable(), unreachable(), unreachable(), unreachable(), ok()]);
    assert_eq!(o.code, 4, "{}", o.out);
    assert_eq!(o.calls.len(), usize::try_from(ATTEMPTS).unwrap());
    assert!(
        o.out.contains("isoloom: site.yml: still unreachable after 4 runs: dc01; giving up"),
        "{}",
        o.out
    );
    assert!(!o.out.contains("after\n"), "set -e stops the setup: {}", o.out);
    cleanup(&o.dir);
}

#[test]
fn a_failed_task_is_not_retried_even_with_a_host_unreachable() {
    // ansible-playbook exits 4 when a host is unreachable even if a task failed on another: the
    // recap's `failed=1` says the failure is real.
    let mixed = run(&[gone("dc01"), fatal("web", "no such user")], &[("dc01", 1, 0), ("web", 0, 1)], 4);
    let o = play(&[mixed, ok()]);
    assert_eq!((o.code, o.calls.len()), (4, 1), "{}", o.out);
    assert!(o.out.contains("isoloom: site.yml: a task failed on web: not retried"), "{}", o.out);
    assert!(!o.out.contains("running it again"), "{}", o.out);
    cleanup(&o.dir);
    // A task that failed on its own (exit 2) isn't either.
    let o = play(&[run(&[fatal("dc01", "no such user")], &[("web", 0, 0), ("dc01", 0, 1)], 2), ok()]);
    assert_eq!((o.code, o.calls.len()), (2, 1), "{}", o.out);
    assert!(!o.out.contains("after"));
    cleanup(&o.dir);
}

#[test]
fn a_playbook_that_does_not_parse_is_not_retried() {
    // ansible-core exits 4 for a syntax error too: no recap.
    let syntax = Run {
        out: "ERROR! We were unable to read either as JSON nor YAML\n".into(),
        code: 4,
    };
    let o = play(&[syntax, ok()]);
    assert_eq!((o.code, o.calls.len()), (4, 1), "{}", o.out);
    assert!(o.out.contains("with no unreachable host or dropped connection: not retried"), "{}", o.out);
    assert!(!o.out.contains("running it again") && !o.out.contains("after"), "{}", o.out);
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
fn a_task_cut_by_a_dropped_connection_runs_again_on_its_host_whatever_the_message() {
    for msg in TRANSIENT_MESSAGES {
        let cut = run(&[fatal("dc01", msg)], &[("web", 0, 0), ("dc01", 0, 1)], 2);
        let o = play(&[cut, ok()]);
        assert_eq!((o.code, o.calls.len()), (0, 2), "{msg}: {}", o.out);
        assert_eq!(o.limits, ["dc01"], "{msg}");
        assert!(
            o.out
                .contains("the play was cut by a dropped connection on dc01; waiting 0 s, then running it again on them (2/4)"),
            "{msg}: {}",
            o.out
        );
        cleanup(&o.dir);
    }
}

#[test]
fn a_play_cut_twice_finishes_on_the_third_run() {
    let cut = || run(&[fatal("dc01", "winrm send_input failed")], &[("web", 0, 0), ("dc01", 0, 1)], 2);
    let o = play(&[cut(), cut(), ok()]);
    assert_eq!(o.code, 0, "{}", o.out);
    assert_eq!(o.calls, [FIRST, AGAIN, AGAIN]);
    assert!(o.out.contains("(3/4)") && o.out.trim_end().ends_with("after"), "{}", o.out);
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
fn mixed_hosts_retry_only_the_cut_and_unreachable_ones() {
    // web got through; dc01's task was cut, dc02 unreachable: only those two run again.
    let mixed = run(
        &[fatal("dc01", "The pipe has been ended."), gone("dc02")],
        &[("web", 0, 0), ("dc01", 0, 1), ("dc02", 1, 0)],
        4,
    );
    let o = play(&[mixed, ok()]);
    assert_eq!((o.code, o.calls.clone()), (0, vec![FIRST.to_string(), AGAIN.to_string()]), "{}", o.out);
    assert_eq!(o.limits, ["dc02 dc01"]);
    assert!(o.out.contains("unreachable: dc02 dc01; waiting 0 s"), "{}", o.out);
    // And a real failure anywhere stops it at once, whatever else was cut.
    let real = run(
        &[fatal("web", "no such user"), fatal("dc01", "The pipe has been ended.")],
        &[("web", 0, 1), ("dc01", 0, 1)],
        2,
    );
    let o2 = play(&[real, ok()]);
    assert_eq!((o2.code, o2.calls.len()), (2, 1), "{}", o2.out);
    assert!(o2.out.contains("a task failed on web: not retried"), "{}", o2.out);
    cleanup(&o.dir);
    cleanup(&o2.dir);
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
fn the_output_is_read_through_colors_and_delegation() {
    let colored = Run {
        out: "\u{1b}[0;31mfatal: [dc01 -> localhost]: FAILED! => {\"msg\": \"Connection refused\"}\u{1b}[0m\nPLAY RECAP ***\n\u{1b}[0;31mdc01\u{1b}[0m : \u{1b}[0;32mok=2\u{1b}[0m changed=0 unreachable=0 \u{1b}[0;31mfailed=1\u{1b}[0m skipped=0\n"
            .into(),
        code: 2,
    };
    let o = play(&[colored, ok()]);
    assert_eq!((o.code, o.calls.len()), (0, 2), "{}", o.out);
    assert_eq!(o.limits, ["dc01"]);
    cleanup(&o.dir);
}

#[test]
fn the_retry_file_is_removed_once_the_play_succeeds() {
    let o = play(&[unreachable(), ok()]);
    assert_eq!(o.code, 0, "{}", o.out);
    assert!(!o.dir.join("site.retry").exists());
    cleanup(&o.dir);
}

#[test]
fn the_transient_list_fits_in_its_single_quotes() {
    assert!(TRANSIENT.iter().all(|t| !t.contains('\'') && *t == t.to_lowercase()));
    assert!(helpers().contains(&format!("_isoloom_attempts={ATTEMPTS}\n")));
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
fn the_script_runs_each_playbook_through_run_play_with_retry_files_on() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lab: 10 }\n    vm: { os: debian-12 }\nprovision:\n  - ansible: ansible/site.yml\n    vars: { greeting: hi there }\n",
    )
    .unwrap();
    let s = script(&spec, &["web".into()]);
    assert!(
        s.contains(&format!(
            "mkdir -p {RETRY_DIR}\nexport ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH={RETRY_DIR}\n"
        )),
        "{s}"
    );
    assert!(s.contains(&helpers()), "{s}");
    assert!(
        s.contains("cd /opt/isoloom/ansible\n[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\nrun_play ansible-playbook -i /etc/isoloom/inventory.ini -e '{\"greeting\":\"hi there\"}' --limit 'web' site.yml\n"),
        "{s}"
    );
}
