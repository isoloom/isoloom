//! Secrets never reach a generated file: with every store's value set on the runner, nothing
//! `isoloom generate` writes (any target's files, the VM targets' and Docker hosts' included,
//! the resolved snapshot) holds one. A CTF lab's flags (one input per objective) are optional,
//! sensitive slots, empty when the runner has no value, and files in the machine, never its
//! environment.

use std::path::{Path, PathBuf};

use crate::testing::{Outcome, isoloom_with, project};

const ENV_VALUE: &str = "env-sentinel-7f3a9c";
const FILE_VALUE: &str = "file-sentinel-41b2e8";
const INPUT_VALUE: &str = "input-sentinel-93d1c6";

const SPEC: &str = "
version: 1
name: secret-lab
networks: { lan: { cidr: 10.70.0.0/24 } }
secrets:
  db-password: { env: SECRET_LAB_DB_PASSWORD }
  api-key: { file: api.key }
inputs: [SECRET_LAB_FLAG]
machines:
  app:
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { image: nginx:alpine }
    files:
      - { path: /run/secrets/db-password, secret: db-password }
      - { path: /etc/app/api.key, secret: api-key, mode: 0400 }
      - { path: /ctf/flag, input: SECRET_LAB_FLAG, mode: 0444 }
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
    os: debian@12
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { image: nginx:alpine, steps: [{ run: db.sh }] }
    files:
      - { path: /run/secrets/db-password, secret: db-password }
      - { path: /etc/app/api.key, secret: api-key, mode: 0400 }
  db:
    os: debian@12
    networks: { lan: 20 }
    app: { steps: [{ run: db.sh }, reboot] }   # a reboot: a VM only
    files: [{ path: /run/secrets/db-password, secret: db-password, uid: 999, mode: 0400 }]
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
    os: windows-server@2022
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { steps: [{ run: setup.ps1 }] }
    files:
      - { path: 'C:\\run\\secrets\\db-password', secret: db-password }
      - { path: 'C:\\app\\api.key', secret: api-key }
    command: [app.exe]
";

/// A CTF lab as the Cyber CTF launcher feeds it: one input per objective (`CTF_FLAG_<KEY>`), each
/// a file, on containers and on a VM.
const FLAGS: &str = "
version: 1
name: flag-lab
networks: { lan: { cidr: 10.73.0.0/24 } }
inputs: [CTF_FLAG_MAIN, CTF_FLAG_SSRF_TO_METADATA, CTF_FLAG_CTF_KEY]
machines:
  web:
    os: debian@12
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { image: nginx:alpine, steps: [{ run: db.sh }] }
    files:
      - { path: /ctf/flag, input: CTF_FLAG_MAIN, mode: 0444 }
      - { path: /ctf/ssrf-flag, input: CTF_FLAG_SSRF_TO_METADATA, mode: 0444 }
      - { path: /srv/app/ctf.key, input: CTF_FLAG_CTF_KEY }
";

/// The flags of [`FLAGS`].
const FLAG_INPUTS: [&str; 3] = ["CTF_FLAG_MAIN", "CTF_FLAG_SSRF_TO_METADATA", "CTF_FLAG_CTF_KEY"];

/// A project holding `spec` and the files it names, the file secret's value among them.
fn secret_project(spec: &str, tag: &str) -> Outcome<PathBuf> {
    let dir = project(&format!("secrets-{tag}"), spec)?;
    std::fs::write(dir.join("api.key"), FILE_VALUE)?;
    std::fs::write(dir.join("db.sh"), "true\n")?;
    std::fs::write(dir.join("setup.ps1"), "exit 0\n")?;
    Ok(dir)
}

/// Every file under `dir`, with its contents.
fn files(dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Outcome {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files(&path, out)?;
        } else {
            let text = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
            out.push((path, text));
        }
    }
    Ok(())
}

/// Every file `isoloom generate` writes for `spec`, once checked that none holds a value. Paths
/// are relative to `.isoloom` and compared by component, so the checks hold on Windows too.
fn generated(spec: &str, tag: &str) -> Outcome<Vec<(PathBuf, String)>> {
    let dir = secret_project(spec, tag)?;
    let home = dir.join("home");
    let env = [
        ("SECRET_LAB_DB_PASSWORD", ENV_VALUE),
        ("SECRET_LAB_FLAG", INPUT_VALUE),
        ("CTF_FLAG_MAIN", INPUT_VALUE),
        ("CTF_FLAG_SSRF_TO_METADATA", INPUT_VALUE),
        ("CTF_FLAG_CTF_KEY", INPUT_VALUE),
    ];
    let o = isoloom_with(&home, &[&"generate", &dir], &env)?;
    if o.code != Some(0) {
        return Err(format!("generate failed: {o:?}").into());
    }
    let root = dir.join(".isoloom");
    let mut generated = Vec::new();
    files(&root, &mut generated)?;
    let mut relative = Vec::new();
    for (path, text) in generated {
        if text.contains(ENV_VALUE) || text.contains(FILE_VALUE) || text.contains(INPUT_VALUE) {
            return Err(format!("{} holds a secret's value", path.display()).into());
        }
        relative.push((path.strip_prefix(&root)?.to_path_buf(), text));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(relative)
}

/// The contents of the file of `generated` ending with `suffix`.
fn find<'a>(generated: &'a [(PathBuf, String)], suffix: &str) -> Outcome<&'a str> {
    let names: Vec<String> = generated.iter().map(|(p, _)| p.display().to_string()).collect();
    let (_, text) = generated
        .iter()
        .find(|(p, _)| p.ends_with(suffix))
        .ok_or_else(|| format!("no {suffix} in {names:?}"))?;
    Ok(text)
}

#[test]
fn no_secret_value_reaches_a_generated_file() -> Outcome {
    let generated = generated(SPEC, "containers")?;
    // Docker and Kubernetes take the spec: their files are there, naming the secrets only.
    let compose = find(&generated, "docker/compose.yml")?;
    assert!(compose.contains("environment: ISOLOOM_SECRET_DB_PASSWORD"), "{compose}");
    let pods = find(&generated, "kubernetes/environment.yaml")?;
    assert!(pods.contains("secretName: isoloom-secrets"), "{pods}");
    find(&generated, "resolved.json")?;
    // The docker-vm host waits for the secrets the runner delivers, then hands them to Compose.
    let dvm = find(&generated, "docker-vm/Vagrantfile")?;
    assert!(
        dvm.contains("/var/lib/isoloom/secrets/.delivered") && dvm.contains("ISOLOOM_SECRET_DB_PASSWORD"),
        "{dvm}"
    );
    // On cloud-docker, Terraform makes the VM only; the run manifest starts Compose over SSH with
    // the containers' secrets forwarded to it (SendEnv), the value in no generated file.
    let run = find(&generated, "cloud-docker/aws/run.json")?;
    assert!(run.contains("ISOLOOM_SECRET_DB_PASSWORD") && run.contains("\"via\": \"env\""), "{run}");
    assert!(!run.contains("/var/lib/isoloom/secrets/.delivered"), "{run}");
    Ok(())
}

#[test]
fn no_secret_value_reaches_a_vm_target_file() -> Outcome {
    let generated = generated(VMS, "vms")?;
    // The VM targets' set-up waits for the secrets the runner delivers.
    for vm in ["vagrant/Vagrantfile", "hybrid/Vagrantfile", "proxmox/main.tf", "cloud-vm/aws/main.tf"] {
        assert!(
            find(&generated, vm)?.contains("/var/lib/isoloom/secrets/.delivered"),
            "{vm} doesn't wait for the secrets"
        );
    }
    Ok(())
}

#[test]
fn no_secret_value_reaches_a_windows_machines_file() -> Outcome {
    let generated = generated(WINDOWS, "windows")?;
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
    assert!(find(&generated, "cloud-vm/aws/main.tf")?.contains(" winrm\\\" > \\\""));
    Ok(())
}

#[test]
fn every_flag_is_an_optional_sensitive_slot_and_a_file_never_an_environment_value() -> Outcome {
    let generated = generated(FLAGS, "flags")?;
    // The targets whose runner delivers a machine's inputs: Compose's secrets, Vagrant's guest
    // delivery. (Proxmox's manifest has no input slot yet: its VMs wait for a delivery no step
    // makes; the clouds have no manifest yet.)
    for target in ["docker", "vagrant"] {
        let path = PathBuf::from(target).join("run.json");
        let text = find(&generated, &format!("{target}/run.json"))?;
        let manifest: serde_json::Value = serde_json::from_str(text)?;
        let slots = manifest["secrets"].as_array().ok_or_else(|| format!("{} has no slots", path.display()))?;
        for flag in FLAG_INPUTS {
            let slot = slots
                .iter()
                .find(|s| s["name"] == flag)
                .ok_or_else(|| format!("{} has no slot {flag}", path.display()))?;
            // The launcher gives a flag only when the backend issues it: without one, the slot
            // is empty (the lab's development value), never refused, never shown.
            assert_eq!(
                (&slot["kind"], &slot["required"], &slot["sensitive"], &slot["default"]),
                (&"input".into(), &false.into(), &true.into(), &"".into()),
                "{} {flag}",
                path.display()
            );
            assert!(
                slot["deliver"].as_array().is_some_and(|d| !d.is_empty()),
                "{} {flag} isn't delivered",
                path.display()
            );
        }
    }
    // On containers, each flag is a secret file read from the runner's environment, never one of
    // the container's variables; on a VM, a secret file the guest delivery writes, never a line of
    // the inputs file the machine's scripts read.
    let compose = find(&generated, "docker/compose.yml")?;
    let vagrantfile = find(&generated, "vagrant/Vagrantfile")?;
    for flag in FLAG_INPUTS {
        assert!(compose.contains(&format!("environment: {flag}\n")), "{flag}: {compose}");
        assert!(!compose.contains(&format!("${{{flag}")), "{flag} is interpolated: {compose}");
        assert!(
            vagrantfile.contains(&format!("\"ISOLOOM_SECRET_INPUT_{flag}\" => isoloom_b64(INPUTS[\"{flag}\"])")),
            "{flag}: {vagrantfile}"
        );
        assert!(
            !vagrantfile.contains(&format!("\"{flag}\" => isoloom_b64")),
            "{flag} is a plain input: {vagrantfile}"
        );
    }
    Ok(())
}
