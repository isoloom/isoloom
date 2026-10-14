//! Secrets never reach a generated file: with every store's value set on the runner, nothing
//! `isoloom generate` writes (any target's files, the VM targets' and Docker hosts' included,
//! the resolved snapshot) holds one.

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

/// VMs, one of them a container on hybrid.
const VMS: &str = "
version: 1
name: secret-vms
networks: { lan: { cidr: 10.71.0.0/24 } }
secrets:
  db-password: { env: SECRET_LAB_DB_PASSWORD }
  api-key: { file: api.key }
machines:
  app:
    networks: { lan: 10 }
    services: [{ port: 80 }]
    docker: { image: nginx:alpine }
    vm: { os: debian-12, provision: [db.sh] }
    secrets: [db-password, { source: api-key, target: /etc/app/api.key, mode: 0400 }]
  db:
    networks: { lan: 20 }
    vm: { os: debian-12, provision: [db.sh] }
    secrets: [{ source: db-password, uid: 999, mode: 0400 }]
";

/// A Windows machine, delivered to over WinRM, with its own process.
const WINDOWS: &str = "
version: 1
name: secret-win
networks: { lan: { cidr: 10.72.0.0/24 } }
secrets:
  db-password: { env: SECRET_LAB_DB_PASSWORD }
  api-key: { file: api.key }
machines:
  win:
    networks: { lan: 10 }
    services: [{ port: 80 }]
    vm: { os: windows-server-2022, provision: [setup.ps1] }
    secrets: [db-password, { source: api-key, target: 'C:\\app\\api.key' }]
    command: [app.exe]
";

fn project(spec: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-secrets-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("isoloom.yml"), spec).unwrap();
    std::fs::write(dir.join("api.key"), FILE_VALUE).unwrap();
    std::fs::write(dir.join("db.sh"), "true\n").unwrap();
    std::fs::write(dir.join("setup.ps1"), "exit 0\n").unwrap();
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

/// Every file `isoloom generate` writes for `spec`, once checked that none holds a value. Paths
/// are relative to `.isoloom` and compared by component, so the checks hold on Windows too.
fn generated(spec: &str, tag: &str) -> Vec<(PathBuf, String)> {
    let dir = project(spec, tag);
    let o = Command::new(env!("CARGO_BIN_EXE_isoloom"))
        .args(["generate", dir.to_str().unwrap()])
        .env("ISOLOOM_HOME", dir.join("home"))
        .env("SECRET_LAB_DB_PASSWORD", ENV_VALUE)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let root = dir.join(".isoloom");
    let mut generated = Vec::new();
    files(&root, &mut generated);
    for (path, _) in &mut generated {
        *path = path.strip_prefix(&root).unwrap().to_path_buf();
    }
    for (path, text) in &generated {
        assert!(
            !text.contains(ENV_VALUE) && !text.contains(FILE_VALUE),
            "{} holds a secret's value",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    generated
}

/// The contents of the file of `generated` ending with `suffix`.
fn find<'a>(generated: &'a [(PathBuf, String)], suffix: &str) -> &'a str {
    let names: Vec<String> = generated.iter().map(|(p, _)| p.display().to_string()).collect();
    &generated
        .iter()
        .find(|(p, _)| p.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix} in {names:?}"))
        .1
}

#[test]
fn no_secret_value_reaches_a_generated_file() {
    let generated = generated(SPEC, "containers");
    // Docker and Kubernetes take the spec: their files are there, naming the secrets only.
    let compose = find(&generated, "docker/compose.yml");
    assert!(compose.contains("environment: ISOLOOM_SECRET_DB_PASSWORD"), "{compose}");
    let pods = find(&generated, "kubernetes/environment.yaml");
    assert!(pods.contains("secretName: isoloom-secrets"), "{pods}");
    find(&generated, "resolved.json");
    // A Docker host waits for the secrets `isoloom run` delivers, then hands them to Compose.
    for host in ["docker-vm/Vagrantfile", "cloud-docker/aws/main.tf"] {
        let text = find(&generated, host);
        assert!(
            text.contains("/var/lib/isoloom/secrets/.delivered") && text.contains("ISOLOOM_SECRET_DB_PASSWORD"),
            "{host}"
        );
    }
}

#[test]
fn no_secret_value_reaches_a_vm_target_file() {
    let generated = generated(VMS, "vms");
    // The VM targets' set-up waits for the secrets `isoloom run` delivers.
    for vm in ["vagrant/Vagrantfile", "hybrid/Vagrantfile", "proxmox/main.tf", "cloud-vm/aws/main.tf"] {
        assert!(
            find(&generated, vm).contains("/var/lib/isoloom/secrets/.delivered"),
            "{vm} doesn't wait for the secrets"
        );
    }
}

#[test]
fn no_secret_value_reaches_a_windows_machines_file() {
    let generated = generated(WINDOWS, "windows");
    // Each VM target's Windows set-up waits for the delivery, then registers the startup task
    // that puts the files back at every boot.
    for target in ["vagrant", "proxmox", "cloud-vm/aws", "cloud-vm/azure"] {
        let set_up = generated
            .iter()
            .filter(|(p, _)| p.starts_with(target))
            .any(|(_, text)| text.contains("secrets\\\\.delivered") || text.contains("secrets\\.delivered"));
        assert!(set_up, "{target} doesn't wait for the secrets");
        let task = generated
            .iter()
            .any(|(p, text)| p.starts_with(target) && text.contains("isoloom-apply-secrets"));
        assert!(task, "{target} doesn't put them back at boot");
    }
    // A cloud's set-up says how to reach its Windows machine over WinRM, never its password.
    assert!(find(&generated, "cloud-vm/aws/main.tf").contains(" winrm\\\" > \\\""));
}
