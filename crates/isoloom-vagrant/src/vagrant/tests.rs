//! The Vagrant backend's unit tests: the retry helpers its controller runs, and the Vagrantfile
//! rendered from small plans.

// The retry helpers run on the Linux controller VM; the test drives them with a POSIX `sh`.
#[cfg(unix)]
mod retry {
    use isoloom_render::RETRIES;

    /// Runs `body` after the retry helpers under `sh -e`, with no pauses; returns (exit code,
    /// output, how many times the fake command ran).
    fn run(body: &str) -> (i32, String, usize) {
        let dir = std::env::temp_dir().join(format!("isoloom-retries-{}-{}", std::process::id(), body.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!("set -e\nCALLS={}/calls\n{RETRIES}{body}\n", dir.display());
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .env("ISOLOOM_RETRY_PAUSE", "0")
            .output()
            .unwrap();
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap_or_default().lines().count();
        std::fs::remove_dir_all(&dir).ok();
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), calls)
    }

    #[test]
    fn a_play_cut_by_a_dropped_connection_runs_again_and_finishes() {
        // Cut by WinRM twice (as under emulation), then it gets through.
        let (code, out, calls) = run(
            "play() { echo x >> $CALLS; [ $(wc -l < $CALLS) -ge 3 ] && echo 'failed=0' || { echo 'fatal: [dc01]: FAILED! => {\"msg\": \"winrm send_input failed\"}'; return 2; }; }\nrun_play play\necho after",
        );
        assert_eq!((code, calls), (0, 3), "{out}");
        assert!(out.contains("running it again (3/4)") && out.trim_end().ends_with("after"), "{out}");
    }

    #[test]
    fn a_task_that_fails_on_its_own_is_not_retried() {
        let (code, out, calls) =
            run("play() { echo x >> $CALLS; echo 'fatal: [dc01]: FAILED! => {\"msg\": \"no such user\"}'; return 2; }\nrun_play play\necho after");
        assert_eq!((code, calls), (2, 1), "set -e stops the setup on the play's own failure: {out}");
        assert!(!out.contains("after"));
    }

    #[test]
    fn a_play_that_keeps_dropping_stops_after_four_runs() {
        let (code, _, calls) = run("play() { echo x >> $CALLS; echo 'UNREACHABLE!'; return 4; }\nrun_play play");
        assert_eq!((code, calls), (4, 4));
    }

    #[test]
    fn a_failed_download_is_tried_again() {
        let (code, out, calls) = run("get() { echo x >> $CALLS; [ $(wc -l < $CALLS) -ge 2 ]; }\nretry_download get\necho after");
        assert_eq!((code, calls), (0, 2), "{out}");
        let (code, _, calls) = run("get() { echo x >> $CALLS; return 1; }\nretry_download get");
        assert_eq!((code, calls), (1, 5));
    }
}
