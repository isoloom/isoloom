//! The retry helpers, driven by a POSIX `sh` (they run on the Linux controller) with a fake
//! `ansible-playbook` that prints a recap and writes the `.retry` file as Ansible does.

#![allow(clippy::unwrap_used)] // test helpers: a failing test is the report.

use std::path::{Path, PathBuf};

use isoloom_spec::shell::quote;

use super::{ATTEMPTS, HELPERS, RETRY_DIR, script};

/// One run of the fake: what it prints, the hosts it writes to `site.retry`, its exit code.
struct Run {
    out: String,
    retry: Option<&'static str>,
    code: i32,
}

/// A recap line per `(host, unreachable, failed)`.
fn recap(hosts: &[(&str, u32, u32)]) -> String {
    let mut out = String::from("PLAY [all] ***\n\nPLAY RECAP *********************************************************************\n");
    for (h, u, f) in hosts {
        out.push_str(&format!(
            "{h:<27}: ok=3    changed=1    unreachable={u}    failed={f}    skipped=0    rescued=0    ignored=0\n"
        ));
    }
    out
}

fn ok() -> Run {
    Run {
        out: recap(&[("web", 0, 0), ("dc01", 0, 0)]),
        retry: None,
        code: 0,
    }
}

fn unreachable() -> Run {
    Run {
        out: recap(&[("web", 0, 0), ("dc01", 1, 0)]),
        retry: Some("dc01"),
        code: 4,
    }
}

fn task_failed() -> Run {
    Run {
        out: format!(
            "fatal: [dc01]: FAILED! => {{\"msg\": \"no such user\"}}\n{}",
            recap(&[("web", 0, 0), ("dc01", 0, 1)])
        ),
        retry: Some("dc01"),
        code: 2,
    }
}

struct Outcome {
    code: i32,
    out: String,
    /// The fake's arguments, one line per attempt.
    calls: Vec<String>,
    dir: PathBuf,
}

/// Runs `run_play site.yml fake -i inv` under `sh -e` with no pauses; the fake answers each
/// attempt with the next of `runs` (then succeeds).
fn play(runs: &[Run], before: &str) -> Outcome {
    let dir = std::env::temp_dir().join(format!("isoloom-playbooks-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cases = String::new();
    for (i, r) in runs.iter().enumerate() {
        let retry = r.retry.map(|h| format!("printf '%s\\n' {} > \"$RETRY\"; ", quote(h))).unwrap_or_default();
        cases.push_str(&format!("    {}) printf '%s' {}; {retry}return {};;\n", i + 1, quote(&r.out), r.code));
    }
    let body = format!(
        "set -e\nCALLS={calls}\nRETRY=\"$ANSIBLE_RETRY_FILES_SAVE_PATH/site.retry\"\n{HELPERS}fake() {{\n  echo \"$*\" >> \"$CALLS\"\n  case $(wc -l < \"$CALLS\" | tr -d ' ') in\n{cases}    *) return 0;;\n  esac\n}}\n{before}run_play site.yml fake -i inv\necho after\n",
        calls = dir.join("calls").display(),
    );
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&body)
        .env("ISOLOOM_RETRY_PAUSE", "0")
        .env("ANSIBLE_RETRY_FILES_SAVE_PATH", &dir)
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

#[test]
fn a_play_failing_on_unreachable_hosts_only_runs_again_on_them() {
    let o = play(&[unreachable(), ok()], "");
    assert_eq!(o.code, 0, "{}", o.out);
    let retry = format!("@{}/site.retry", o.dir.display());
    assert_eq!(o.calls, ["-i inv site.yml".to_string(), format!("-i inv site.yml --limit {retry}")]);
    assert!(
        o.out.contains("isoloom: site.yml: unreachable: dc01; waiting 0 s, then attempt 2/3 on them"),
        "{}",
        o.out
    );
    assert!(o.out.trim_end().ends_with("after"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn a_task_failure_is_never_retried() {
    let o = play(&[task_failed(), ok()], "");
    assert_eq!((o.code, o.calls.len()), (2, 1), "set -e stops the setup on the play's failure: {}", o.out);
    assert!(o.out.contains("failed (exit 2, recap: failed): not retried"), "{}", o.out);
    assert!(!o.out.contains("after"));
    cleanup(&o.dir);
}

#[test]
fn a_task_failure_beside_an_unreachable_host_is_not_retried() {
    let mixed = Run {
        out: recap(&[("web", 0, 1), ("dc01", 1, 0)]),
        retry: Some("web\ndc01"),
        code: 2,
    };
    let o = play(&[mixed, ok()], "");
    assert_eq!((o.code, o.calls.len()), (2, 1), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn hosts_that_stay_unreachable_give_up_after_the_last_attempt() {
    let o = play(&[unreachable(), unreachable(), unreachable(), ok()], "");
    assert_eq!(o.code, 4, "{}", o.out);
    assert_eq!(o.calls.len(), usize::try_from(ATTEMPTS).unwrap());
    assert!(o.out.contains("hosts still unreachable after 3 attempts: dc01"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn no_retry_file_or_no_recap_means_no_retry() {
    let no_file = Run { retry: None, ..unreachable() };
    let o = play(&[no_file, ok()], "");
    assert_eq!((o.code, o.calls.len()), (4, 1), "{}", o.out);
    cleanup(&o.dir);
    let syntax = Run {
        out: "ERROR! the playbook: site.yml could not be found\n".into(),
        retry: None,
        code: 1,
    };
    let o = play(&[syntax, ok()], "");
    assert_eq!((o.code, o.calls.len()), (1, 1), "{}", o.out);
    assert!(o.out.contains("recap: none"), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn a_stale_retry_file_is_not_read() {
    // A retry file left by an earlier run, then a run that fails on a task without one: the old
    // hosts must not turn it into a retry.
    let failed = Run { retry: None, ..unreachable() };
    let o = play(&[failed, ok()], "printf 'old\\n' > \"$RETRY\"\n");
    assert_eq!((o.code, o.calls.len()), (4, 1), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn the_recap_is_read_through_colors_and_counted_per_host() {
    let colored = "PLAY RECAP ***\n\u{1b}[0;33mweb\u{1b}[0m : \u{1b}[0;32mok=2\u{1b}[0m changed=0 \u{1b}[1;31munreachable=1\u{1b}[0m failed=0 skipped=0\n";
    let o = play(
        &[
            Run {
                out: colored.into(),
                retry: Some("web"),
                code: 4,
            },
            ok(),
        ],
        "",
    );
    assert_eq!((o.code, o.calls.len()), (0, 2), "{}", o.out);
    cleanup(&o.dir);
}

#[test]
fn a_failed_download_is_tried_again() {
    let run = |body: &str| {
        let s = format!("set -e\n{HELPERS}n=0\nget() {{ n=$((n + 1)); echo $n > /dev/null; {body}; }}\nretry_download get\necho \"calls=$n\"\n");
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
        s.contains(&format!("ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH={RETRY_DIR}\n")),
        "{s}"
    );
    assert!(s.contains(&format!("mkdir -p /tmp/isoloom-facts {RETRY_DIR}\n")), "{s}");
    assert!(
        s.contains("cd /opt/isoloom/ansible\n[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\nrun_play 'site.yml' ansible-playbook -i /etc/isoloom/inventory.ini -e '{\"greeting\":\"hi there\"}' --limit 'web'\n"),
        "{s}"
    );
    assert!(HELPERS.contains(&format!("_isoloom_attempts={ATTEMPTS}\n")));
}
