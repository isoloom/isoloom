//! Secrets never reach a generated file: with every store's value set on the runner, nothing
//! `isoloom generate` writes (any target's files, the resolved snapshot) holds one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};
use std::process::Command;

const ENV_VALUE: &str = "env-sentinel-7f3a9c";
const FILE_VALUE: &str = "file-sentinel-41b2e8";

const SPEC: &str = "
version: 1
name: secret-lab
networks: { lan: { cidr: 10.70.0.0/24 } }
secrets:
  db-password: { env: SECRET_LAB_DB_PASSWORD }
  api-key: { file: api.key }
machines:
  app:
    networks: { lan: 10 }
    services: [{ port: 80 }]
    docker: { image: nginx:alpine }
    secrets: [db-password, { source: api-key, target: /etc/app/api.key, mode: 0400 }]
";

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-secrets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("isoloom.yml"), SPEC).unwrap();
    std::fs::write(dir.join("api.key"), FILE_VALUE).unwrap();
    dir
}

/// Every file under `dir`, with its contents.
fn files(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push((path.clone(), String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned()));
        }
    }
}

#[test]
fn no_secret_value_reaches_a_generated_file() {
    let dir = project();
    let o = Command::new(env!("CARGO_BIN_EXE_isoloom"))
        .args(["generate", dir.to_str().unwrap()])
        .env("ISOLOOM_HOME", dir.join("home"))
        .env("SECRET_LAB_DB_PASSWORD", ENV_VALUE)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let mut generated = Vec::new();
    files(&dir.join(".isoloom"), &mut generated);
    let names: Vec<String> = generated.iter().map(|(p, _)| p.display().to_string()).collect();
    // Docker and Kubernetes take the spec: their files are there, naming the secrets only.
    let compose = generated.iter().find(|(p, _)| p.ends_with("docker/compose.yml")).expect("a Compose file");
    assert!(compose.1.contains("environment: ISOLOOM_SECRET_DB_PASSWORD"), "{}", compose.1);
    let pods = generated
        .iter()
        .find(|(p, _)| p.ends_with("kubernetes/environment.yaml"))
        .expect("Kubernetes manifests");
    assert!(pods.1.contains("secretName: isoloom-secrets"), "{}", pods.1);
    assert!(names.iter().any(|n| n.ends_with("resolved.json")), "{names:?}");
    for (path, text) in &generated {
        assert!(
            !text.contains(ENV_VALUE) && !text.contains(FILE_VALUE),
            "{} holds a secret's value",
            path.display()
        );
    }
}
