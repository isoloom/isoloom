use proptest::prelude::*;

use super::CheckLine;

fn pass(name: &str) -> CheckLine {
    CheckLine::Pass { name: name.into() }
}

fn fail(name: &str, reason: &str) -> CheckLine {
    CheckLine::Fail {
        name: name.into(),
        reason: reason.into(),
    }
}

/// Text a tool may print before a runner's line: anything without the runner's marker.
fn prefix() -> impl Strategy<Value = String> {
    "[ -~]{0,12}".prop_filter("no marker", |p| !p.contains("isoloom-check: "))
}

/// A check's name: printable, no `: ` (a FAIL line's separator), nothing trailing to trim.
fn name() -> impl Strategy<Value = String> {
    "[!-~]([ -~]{0,20}[!-~])?".prop_filter("no separator", |n| !n.contains(": "))
}

proptest! {
    #[test]
    fn a_pass_line_reads_back_its_name(before in prefix(), n in name()) {
        prop_assert_eq!(CheckLine::parse(&format!("{before}isoloom-check: PASS {n}  ")), Some(pass(&n)));
    }

    #[test]
    fn a_fail_line_reads_back_its_name_and_reason(before in prefix(), n in name(), why in "[ -~]{0,30}") {
        prop_assert_eq!(CheckLine::parse(&format!("{before}isoloom-check: FAIL {n}: {why}")), Some(fail(&n, why.trim_end())));
    }

    #[test]
    fn an_end_line_reads_back_its_counts(before in prefix(), passed in any::<u32>(), failed in any::<u32>()) {
        let line = format!("{before}isoloom-check: END {passed} passed, {failed} failed");
        prop_assert_eq!(CheckLine::parse(&line), Some(CheckLine::End { passed, failed }));
    }

    #[test]
    fn a_written_line_reads_back_as_itself(n in name(), why in "([!-~]([ -~]{0,20}[!-~])?)?", passed in any::<u32>(), failed in any::<u32>()) {
        for line in [pass(&n), fail(&n, &why), CheckLine::End { passed, failed }] {
            prop_assert_eq!(CheckLine::parse(&line.to_string()), Some(line));
        }
    }

    #[test]
    fn a_line_without_the_marker_is_none(line in prefix()) {
        prop_assert_eq!(CheckLine::parse(&line), None);
    }

    #[test]
    fn any_line_reads_without_a_panic(line in "\\PC{0,120}") {
        let _line = CheckLine::parse(&line);
    }
}

#[test]
fn a_line_is_read_after_the_prefix_a_tool_prints() {
    assert_eq!(CheckLine::parse("isoloom-check: PASS web:80 from user"), Some(pass("web:80 from user")));
    assert_eq!(
        CheckLine::parse("web-1  | isoloom-check: FAIL secret readable: 404"),
        Some(fail("secret readable", "404"))
    );
    assert_eq!(
        CheckLine::parse("    web: isoloom-check: FAIL cache:6379 from web: nothing listens at 10.61.20.20:6379"),
        Some(fail("cache:6379 from web", "nothing listens at 10.61.20.20:6379"))
    );
    assert_eq!(CheckLine::parse("== checks/x.sh"), None);
}

#[test]
fn a_fail_line_without_a_reason_has_an_empty_one() {
    assert_eq!(CheckLine::parse("isoloom-check: FAIL web"), Some(fail("web", "")));
    assert_eq!(CheckLine::parse("isoloom-check: FAIL web  "), Some(fail("web", "")));
    assert_eq!(CheckLine::parse("isoloom-check: FAIL web  : down"), Some(fail("web", "down")));
}

#[test]
fn only_the_three_kinds_with_their_parts_are_lines() {
    assert_eq!(CheckLine::parse("isoloom-check: SKIP web"), None);
    assert_eq!(CheckLine::parse("isoloom-check: ENDING"), None);
    // An END line without both counts isn't the runner's end: a runner cut off mid-line is
    // unfinished.
    assert_eq!(CheckLine::parse("isoloom-check: END 3 passed"), None);
    assert_eq!(CheckLine::parse("isoloom-check: END "), None);
}
