//! The `isoloom` binary itself, on paths that need no Docker or VMs: argument errors, a missing
//! or invalid spec, and the registry behind `status`. Each test gets its own `ISOLOOM_HOME`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh, empty folder under the system temp dir, unique to this test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `isoloom <args>` with `home` as its state folder.
fn isoloom(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_isoloom"))
        .args(args)
        .env("ISOLOOM_HOME", home)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn example(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name).display().to_string()
}

/// A project folder holding `yaml` as its isoloom.yml.
fn project(name: &str, yaml: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("isoloom.yml"), yaml).unwrap();
    dir
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let home = scratch("unknown-subcommand");
    let o = isoloom(&home, &["frobnicate"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("unrecognized subcommand"), "{}", stderr(&o));
}

#[test]
fn missing_spec_is_an_error() {
    let home = scratch("missing-spec");
    let o = isoloom(&home, &["validate", home.join("nowhere").to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("no isoloom.yml"), "{}", stderr(&o));
}

#[test]
fn an_example_validates() {
    let home = scratch("example-validates");
    let o = isoloom(&home, &["validate", &example("hello-stack")]);
    assert!(o.status.success(), "{}{}", stdout(&o), stderr(&o));
    assert!(stdout(&o).contains("hello-stack is valid"), "{}", stdout(&o));
}

#[test]
fn invalid_spec_fails_validate_with_its_problems() {
    let dir = project(
        "invalid-spec",
        "version: 1\nname: broken\nnetworks:\n  lan: { cidr: 10.0.0.0/24 }\nmachines:\n  a:\n    networks: { wan: 5 }\n    docker: { image: alpine }\n",
    );
    let o = isoloom(&dir, &["validate", dir.to_str().unwrap(), "--json"]);
    assert_eq!(o.status.code(), Some(1), "{}{}", stdout(&o), stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["ok"], false);
    assert!(!v["problems"].as_array().unwrap().is_empty());
}

#[test]
fn run_rejects_an_unknown_target() {
    let home = scratch("run-unknown-target");
    let o = isoloom(&home, &["run", "bogus", &example("hello-stack")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("unknown target `bogus`"), "{}", stderr(&o));
}

#[test]
fn exec_needs_a_command() {
    let home = scratch("exec-no-command");
    let o = isoloom(&home, &["exec", "web", &example("hello-stack")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("give the command after `--`"), "{}", stderr(&o));
}

#[test]
fn status_with_nothing_running() {
    let home = scratch("status-empty");
    let o = isoloom(&home, &["status"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("no environment is up"), "{}", stdout(&o));
    let o = isoloom(&home, &["status", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["environments"], serde_json::json!([]));
}

#[test]
fn status_cleanup_of_an_unknown_name_fails() {
    let home = scratch("status-cleanup-unknown");
    let o = isoloom(&home, &["status", "--cleanup", "nope"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("no environment named `nope`"), "{}", stderr(&o));
}

/// A registry entry whose project folder is gone, as `run` would have written it.
fn write_stale_entry(home: &Path, name: &str) {
    let gone = home.join("gone-project");
    let yaml = format!(
        "environments:\n- name: {name}\n  dir: {}\n  target: docker\n  started: 2026-01-01T00:00:00Z\n",
        gone.display()
    );
    std::fs::write(home.join("status.yml"), yaml).unwrap();
}

#[test]
fn status_reports_an_environment_whose_folder_is_gone() {
    let home = scratch("status-stale");
    write_stale_entry(&home, "lost");
    let o = isoloom(&home, &["status", "--json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["environments"][0]["name"], "lost");
    assert_eq!(v["environments"][0]["state"], "stale (folder gone)");
}

#[test]
fn status_cleanup_forgets_a_stale_environment() {
    let home = scratch("status-cleanup-stale");
    write_stale_entry(&home, "lost");
    let o = isoloom(&home, &["status", "--cleanup", "lost"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = isoloom(&home, &["status", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["environments"], serde_json::json!([]));
}
