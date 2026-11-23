//! Guardrails over the tests' own sources, for what makes a test pass or fail by luck
//! (CONTRIBUTING, "Tests"):
//!
//! - a test waits on a condition or an event, never on the clock: the `sleep`s in the tests are
//!   counted per file ([`SLEEPS`]), each count with why those are no wait for something (a
//!   stand-in that answers late, the poll of a bounded wait), so a new one fails;
//! - a free port is held until it is used: a listener bound to port 0 then let go (`drop(l)`, or a
//!   `.local_addr()` read off a listener the statement doesn't keep) hands its port to whichever
//!   process asks next ([`PORTS_LET_GO`] lists the tests whose subject is a port let go).
//!
//! The tests' sources: every `.rs` file under `crates/` and `xtask/` that is a test's whole (a
//! `tests/` folder, `tests.rs`, `testing.rs`, `test_support.rs`, the test-support crate), and the
//! rest of a file from its inline `#[cfg(test)] mod tests {` on.

use super::walk;
use crate::repo;

/// The `sleep`s each file's tests have, and why none of them waits for something to happen.
const SLEEPS: [(&str, usize, &str); 2] = [
    (
        "crates/tools/isoloom-test-support/src/lib.rs",
        2,
        "the poll of a bounded wait (500 looks) for a helper's pid file, then for the helper to be gone",
    ),
    (
        "crates/targets/isoloom-targets/tests/targets/checks/probes.rs",
        1,
        "the stand-in service answering `delay_ms` late: the lateness is what the check under test measures",
    ),
];

/// The files whose tests let a bound port go on purpose, and why.
const PORTS_LET_GO: [(&str, usize, &str); 0] = [];

/// One test source: its path from the repository root (with `/`), its test code, and the number
/// of lines before it in the file.
struct TestSource {
    path: String,
    text: String,
    skipped: usize,
}

/// Every test source under `crates/` and `xtask/`, sorted.
fn test_sources() -> Vec<TestSource> {
    let root = repo::root();
    let mut files = Vec::new();
    for dir in ["crates", "xtask"] {
        walk(&root.join(dir), &mut files);
    }
    let mut out: Vec<TestSource> = files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
        .filter_map(|f| {
            let path = f.strip_prefix(&root).ok()?.to_string_lossy().replace('\\', "/");
            let name = f.file_name()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&f).ok()?;
            let whole = path.contains("/tests/")
                || ["tests.rs", "testing.rs", "test_support.rs"].contains(&name.as_str())
                || path.starts_with("crates/tools/isoloom-test-support/");
            let from = if whole { 0 } else { test_module(&text)? };
            let skipped = text.get(..from).unwrap_or_default().matches('\n').count();
            Some(TestSource {
                path,
                text: text.get(from..).unwrap_or_default().to_owned(),
                skipped,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Where `text`'s inline test module starts: the first `#[cfg(test)]` over a `mod … {` (one over
/// `mod tests;` declares a file that is a test source of its own).
fn test_module(text: &str) -> Option<usize> {
    text.match_indices("#[cfg(test)]").map(|(at, _)| at).find(|at| {
        text.get(*at..)
            .and_then(|rest| rest.lines().nth(1))
            .is_some_and(|next| next.trim_start().starts_with("mod ") && next.trim_end().ends_with('{'))
    })
}

/// The lines (from 1) of `text` calling a `sleep`: `thread::sleep`, `tokio::time::sleep`, or one
/// imported (`sleep(`), comments and a function's own definition left out.
fn sleeps(text: &str) -> Vec<usize> {
    let calls = |l: &str| {
        l.match_indices("sleep(")
            .filter(|(at, _)| !l.get(..*at).is_some_and(|before| before.ends_with("fn ") || before.ends_with('_')))
            .count()
    };
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
        .flat_map(|(i, l)| std::iter::repeat_n(i + 1, calls(l)))
        .collect()
}

/// Whether `line` binds a socket to port 0 (the system picks a free one).
fn binds_port_zero(line: &str) -> bool {
    line.contains("bind(") && (line.contains(":0\"") || line.contains(", 0))") || line.contains(",0))"))
}

/// How many lines after the bind a port let go is looked for: bound, its port read, dropped.
const LET_GO_WITHIN: usize = 4;

/// The lines (from 1) of `text` letting a port the system picked go before it is used: a
/// `let <name> = …bind(…0…)` whose `<name>` is dropped within the next few lines (its port read
/// in between), or a `.local_addr()` read off a bound listener the statement doesn't keep.
fn ports_let_go(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (i, line) in lines
        .iter()
        .enumerate()
        .filter(|(_, l)| binds_port_zero(l) && !l.trim_start().starts_with("//"))
    {
        let name = line
            .trim_start()
            .strip_prefix("let ")
            .map(|rest| rest.trim_start_matches("mut "))
            .and_then(|rest| rest.split([' ', ':', '=']).next())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'));
        // The lines a statement chained over a few lines takes.
        let statement: String = lines
            .get(i..)
            .unwrap_or_default()
            .iter()
            .take(LET_GO_WITHIN)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        // From the bind to the statement's end (its `;`, or the block it opens).
        let read_off = statement
            .split_once("bind(")
            .is_some_and(|(_, after)| after.split([';', '{']).next().unwrap_or_default().contains(".local_addr()"));
        let next = lines.get(i + 1..).unwrap_or_default().iter().take(LET_GO_WITHIN);
        let dropped = name.is_some_and(|n| next.clone().any(|l| l.contains(&format!("drop({n})"))));
        if read_off || dropped {
            found.push(i + 1);
        }
    }
    found
}

/// What `rule` finds in each test source against its `allowed` count, as the failures to report:
/// a file with more (or fewer) than its count.
fn counted(rule: fn(&str) -> Vec<usize>, allowed: &[(&str, usize, &str)], list: &str, advice: &str) -> Vec<String> {
    let mut wrong = Vec::new();
    // The scans' own cases below are no test's.
    for s in test_sources().into_iter().filter(|s| s.path != "xtask/src/guardrails/flakes.rs") {
        let lines = rule(&s.text);
        let allowed = allowed.iter().find(|(p, ..)| *p == s.path).map_or(0, |(_, n, _)| *n);
        if lines.len() != allowed {
            let at: Vec<String> = lines.iter().map(|l| (l + s.skipped).to_string()).collect();
            wrong.push(format!(
                "{}: {} found, {allowed} allowed (xtask/src/guardrails/flakes.rs, {list}): {advice}, or lower the count when one went away (lines {})",
                s.path,
                lines.len(),
                at.join(" ")
            ));
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_waits_on_a_condition_not_on_the_clock() {
        let wrong = counted(
            sleeps,
            &SLEEPS,
            "SLEEPS",
            "wait on the condition or event itself (a channel, a child's exit, a poll to a deadline)",
        );
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn a_test_holds_the_free_port_it_picked_until_it_uses_it() {
        let wrong = counted(
            ports_let_go,
            &PORTS_LET_GO,
            "PORTS_LET_GO",
            "keep the listener and hand it (or its port, while it is held) to what uses it",
        );
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn the_flake_rules_see_what_they_look_for() {
        let sleep = "std::thread::sleep(Duration::from_secs(1));\n// thread::sleep(x)\ntokio::time::sleep(d).await;\nsleep(d).await;\nfn sleep(d: u64) {}\ntokio::time::sleep_until(t).await;\n";
        assert_eq!(sleeps(sleep), [1, 3, 4]);
        let ports = [
            (
                "let l = TcpListener::bind(\"127.0.0.1:0\")?;\nlet port = l.local_addr()?.port();\ndrop(l);\n",
                vec![1],
            ),
            (
                "let port = TcpListener::bind(\"127.0.0.1:0\")\n    .unwrap()\n    .local_addr()\n    .unwrap()\n    .port();\n",
                vec![1],
            ),
            ("let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;\nserve(l);\n", vec![]),
            ("let held = TcpListener::bind((\"127.0.0.1\", port))?;\ndrop(held);\n", vec![]),
            (
                "if let Ok(l) = TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)) {\n    let port = l.local_addr().unwrap().port();\n",
                vec![],
            ),
            (
                "let l = TcpListener::bind(\"127.0.0.1:0\")?;\nlet port = l.local_addr()?.port();\nlet c = connect(port);\nuse_it(c);\nserve(&l);\ndrop(l);\n",
                vec![],
            ),
        ];
        for (code, expected) in ports {
            assert_eq!(ports_let_go(code), expected, "{code}");
        }
    }
}
