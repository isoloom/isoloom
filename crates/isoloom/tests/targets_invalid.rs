//! `isoloom targets` on a spec that parses but is invalid: the problems and exit code 1, as
//! `generate` and `check` do, not a panic (exit code 101).

use std::path::PathBuf;
use std::process::Command;

/// A fresh project folder holding `yaml` as its isoloom.yml.
fn project(name: &str, yaml: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-targets-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("isoloom.yml"), yaml).expect("write spec");
    dir
}

/// Runs `isoloom targets <dir>`: its exit code and stderr.
fn targets(dir: &PathBuf) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_isoloom")).arg("targets").arg(dir).output().expect("runs");
    (out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn assert_reports(name: &str, yaml: &str, problem: &str) {
    let dir = project(name, yaml);
    let (code, stderr) = targets(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains(problem), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn an_invalid_cidr_is_reported() {
    let yaml = "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/33 }\nmachines:\n  web: { networks: { app: 10 }, docker: { image: nginx } }\n";
    assert_reports("cidr", yaml, "networks.app.cidr");
}

#[test]
fn an_undeclared_network_is_reported() {
    let yaml = "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, docker: { image: nginx } }\n";
    assert_reports("network", yaml, "no network named `lan`");
}
