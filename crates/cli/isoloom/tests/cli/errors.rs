//! How an error reads and the exit code it gives, the same for every command: 2 when a command
//! can't do its job (its error, then each cause under it), 1 when the spec has problems (each
//! with its field, reason and fix), nothing and 0 when its reader stops reading, never a panic.

use crate::testing::{Outcome, example, isoloom, isoloom_with, project, scratch};

/// A spec that parses but is wrong: a block that isn't one.
const BAD_CIDR: &str = "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/33 }\nmachines:\n  web: { networks: { app: 10 }, app: { image: nginx } }\n";

/// A spec in the earlier format: refused with the fix.
const OLD_FORMAT: &str =
    "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  web: { networks: { app: 10 }, docker: { image: nginx } }\n";

#[test]
fn an_unknown_subcommand_is_a_usage_error() -> Outcome {
    let home = scratch("unknown-subcommand")?;
    let o = isoloom(&home, &[&"frobnicate"])?;
    assert_eq!(o.code, Some(2));
    assert!(o.stderr.contains("unrecognized subcommand"), "{o:?}");
    Ok(())
}

#[test]
fn a_missing_spec_is_an_error() -> Outcome {
    let home = scratch("missing-spec")?;
    let o = isoloom(&home, &[&"validate", &home.join("nowhere")])?;
    assert_eq!(o.code, Some(2));
    assert!(o.stderr.starts_with("error: no isoloom.yml in"), "{o:?}");
    Ok(())
}

#[test]
fn an_unknown_target_is_named() -> Outcome {
    let home = scratch("generate-unknown-target")?;
    let o = isoloom(&home, &[&"generate", &example("hello-stack"), &"--target", &"bogus"])?;
    assert_eq!(o.code, Some(2));
    assert_eq!(o.stderr, "error: unknown target `bogus`\n");
    Ok(())
}

#[test]
fn every_command_that_needs_a_valid_spec_shows_its_problems_and_gives_1() -> Outcome {
    let dir = project("bad-cidr", BAD_CIDR)?;
    for command in ["generate", "check", "targets", "graph", "report", "inspect", "message"] {
        let o = if command == "report" {
            isoloom(&dir, &[&command, &"addressing", &dir])?
        } else {
            isoloom(&dir, &[&command, &dir])?
        };
        assert_eq!(o.code, Some(1), "{command}: {o:?}");
        assert!(o.stderr.contains("✗ error[") && o.stderr.contains("networks.app.cidr"), "{command}: {o:?}");
        assert!(o.stderr.ends_with("error: fix the spec first (`isoloom validate`)\n"), "{command}: {o:?}");
        assert!(!o.stderr.contains("panicked"), "{command}: {o:?}");
    }
    Ok(())
}

#[test]
fn a_spec_in_the_earlier_format_shows_where_why_and_the_fix() -> Outcome {
    let dir = project("old-format", OLD_FORMAT)?;
    let o = isoloom(&dir, &[&"generate", &dir])?;
    assert_eq!(o.code, Some(1), "{o:?}");
    assert!(o.stderr.contains("✗ error[removed-field] machines.web.docker  (isoloom.yml:"), "{o:?}");
    assert!(o.stderr.contains("\n  fix: "), "{o:?}");
    Ok(())
}

#[test]
fn a_file_that_cant_be_read_shows_the_systems_reason_under_the_error() -> Outcome {
    let dir = scratch("unreadable")?;
    let missing = dir.join("compose.yaml");
    let o = isoloom(&dir, &[&"import", &"compose", &missing])?;
    assert_eq!(o.code, Some(2), "{o:?}");
    let mut lines = o.stderr.lines();
    assert_eq!(lines.next(), Some(format!("error: can't read {}", missing.display()).as_str()), "{o:?}");
    assert!(lines.next().is_some_and(|l| l.starts_with("  caused by: ")), "{o:?}");
    Ok(())
}

#[test]
fn a_file_that_isnt_its_format_shows_the_parsers_reason_under_the_error() -> Outcome {
    let dir = scratch("not-compose")?;
    let file = dir.join("compose.yaml");
    std::fs::write(&file, "services: [")?;
    let o = isoloom(&dir, &[&"import", &"compose", &file, &"--stdout"])?;
    assert_eq!(o.code, Some(2), "{o:?}");
    assert!(o.stderr.starts_with("error: not a Compose file\n  caused by: "), "{o:?}");
    Ok(())
}

#[test]
fn a_tool_that_isnt_installed_says_what_to_install_and_what_was_kept() -> Outcome {
    let home = scratch("no-d2")?;
    let out = home.join("lab.svg");
    let o = isoloom_with(&home, &[&"graph", &example("hello-stack"), &"-o", &out], &[("PATH", "")])?;
    assert_eq!(o.code, Some(2), "{o:?}");
    assert!(o.stderr.starts_with("error: `d2` isn't installed, so the diagram isn't rendered"), "{o:?}");
    assert!(o.stderr.contains("https://d2lang.com/tour/install"), "{o:?}");
    assert!(home.join("lab.d2").is_file(), "the source is kept");
    Ok(())
}

#[test]
fn output_its_reader_stops_reading_ends_quietly() -> Outcome {
    use std::process::{Command, Stdio};
    let home = scratch("closed-stdout")?;
    // The schema is larger than a pipe holds, so the command is still writing when the pipe closes.
    let mut child = Command::new(env!("CARGO_BIN_EXE_isoloom"))
        .arg("schema")
        .env("ISOLOOM_HOME", &home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    let o = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(0), "{stderr}");
    assert_eq!(stderr, "");
    Ok(())
}
