//! Secrets never reach a generated file: with every store's value set on the runner, nothing
//! `isoloom generate` writes (any target's files, the VM targets' and Docker hosts' included,
//! the resolved snapshot) holds one. Inputs read as files are optional, sensitive slots, empty
//! when the runner has no value, and files in the machine, never its environment.

use std::path::{Path, PathBuf};

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_text, ruby_blocks, yaml, yaml_documents, yaml_keys_at, yaml_str, yaml_values};

use crate::testing::{Outcome, Scratch, isoloom_with, project};

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
inputs: [DEMO_SECRET_LAB]
machines:
  app:
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { image: nginx:alpine }
    files:
      - { path: /run/secrets/db-password, secret: db-password }
      - { path: /etc/app/api.key, secret: api-key, mode: 0400 }
      - { path: /srv/demo/secret, input: DEMO_SECRET_LAB, mode: 0444 }
";

/// VMs: `app` has a container form too (its image), which the VM targets give as a VM.
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

/// An environment whose runner issues one secret value per input (`DEMO_SECRET_<KEY>`), each
/// a file, on containers and on a VM.
const ISSUED: &str = "
version: 1
name: issued-secrets
networks: { lan: { cidr: 10.73.0.0/24 } }
inputs: [DEMO_SECRET, DEMO_SECRET_API, DEMO_SECRET_KEY]
machines:
  web:
    os: debian@12
    networks: { lan: 10 }
    ports: { web: 80 }
    app: { image: nginx:alpine, steps: [{ run: db.sh }] }
    files:
      - { path: /srv/demo/secret, input: DEMO_SECRET, mode: 0444 }
      - { path: /srv/demo/api-secret, input: DEMO_SECRET_API, mode: 0444 }
      - { path: /srv/app/demo.key, input: DEMO_SECRET_KEY }
";

/// The inputs of [`ISSUED`].
const SECRET_INPUTS: [&str; 3] = ["DEMO_SECRET", "DEMO_SECRET_API", "DEMO_SECRET_KEY"];

/// A project holding `spec` and the files it names, the file secret's value among them.
fn secret_project(spec: &str, tag: &str) -> Outcome<Scratch> {
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
        ("DEMO_SECRET_LAB", INPUT_VALUE),
        ("DEMO_SECRET", INPUT_VALUE),
        ("DEMO_SECRET_API", INPUT_VALUE),
        ("DEMO_SECRET_KEY", INPUT_VALUE),
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
    let compose = yaml(find(&generated, "docker/compose.yml")?);
    assert_eq!(yaml_str(&compose, "secrets.db-password.environment"), "ISOLOOM_SECRET_DB_PASSWORD");
    let pods = yaml_documents(find(&generated, "kubernetes/environment.yaml")?);
    let secrets: Vec<String> = pods.iter().flat_map(|d| yaml_values(d, "secretName")).collect();
    assert!(secrets.iter().any(|s| s == "isoloom-secrets"), "{secrets:?}");
    find(&generated, "resolved.json")?;
    // On cloud-docker, the module takes the containers' secrets as sensitive variables, the
    // value in no generated file.
    let module = hcl_body(find(&generated, "cloud-docker/aws/main.tf")?);
    let variables = hcl_blocks(&module, "variable", &["ISOLOOM_SECRET_DB_PASSWORD"]);
    let sensitive: Vec<String> = variables.iter().map(|v| hcl_text(v.body(), "sensitive")).collect();
    assert_eq!(sensitive, ["true"], "one sensitive variable");
    Ok(())
}

#[test]
fn no_secret_value_reaches_a_vm_target_file() -> Outcome {
    let generated = generated(VMS, "vms")?;
    // Every VM target keeps the values the way its tool hands them over: Vagrant's provisioner
    // environment, the Terraform targets' module variables (cloud user data, Proxmox cloud-init),
    // the secret's name in the files, never its value.
    for vm in ["vagrant/Vagrantfile", "cloud-vm/aws/main.tf", "proxmox/main.tf"] {
        assert!(
            find(&generated, vm)?.contains("/var/lib/isoloom/secrets/.delivered"),
            "{vm} doesn't keep the secrets"
        );
    }
    Ok(())
}

#[test]
fn no_secret_value_reaches_a_windows_machines_file() -> Outcome {
    let generated = generated(WINDOWS, "windows")?;
    // Vagrant's Windows set-up waits for the delivery, then registers the startup task that puts
    // the files back at every boot. The clouds refuse a Windows machine's secrets: its set-up runs
    // during Terraform's apply, before the runner delivers them.
    for cloud in ["aws", "azure"] {
        assert!(find(&generated, &format!("cloud-vm/{cloud}/main.tf")).is_err(), "{cloud} refuses them");
    }
    let set_up = generated
        .iter()
        .filter(|(p, _)| p.starts_with("vagrant"))
        .any(|(_, text)| text.contains("secrets\\\\.delivered") || text.contains("secrets\\.delivered"));
    assert!(set_up, "vagrant doesn't wait for the secrets");
    let task = generated
        .iter()
        .any(|(p, text)| p.starts_with("vagrant") && text.contains("isoloom-apply-secrets"));
    assert!(task, "vagrant doesn't put them back at boot");
    Ok(())
}

#[test]
fn every_issued_input_is_a_file_never_an_environment_value() -> Outcome {
    let generated = generated(ISSUED, "issued")?;
    // On containers, each input is a secret file read from the environment, never one of
    // the container's variables; on a VM, a secret file the guest delivery writes, never a line of
    // the inputs file the machine's scripts read.
    let compose_text = find(&generated, "docker/compose.yml")?;
    let compose = yaml(compose_text);
    let vagrantfile = ruby_blocks(find(&generated, "vagrant/Vagrantfile")?);
    let delivery = vagrantfile
        .block("config.vm.define \"web\"")
        .statements("m.vm.provision \"shell\", name: \"secrets\"");
    let [delivery] = delivery[..] else {
        panic!("one secrets provisioner: {delivery:?}")
    };
    let read_from: Vec<String> = yaml_keys_at(&compose, "secrets")
        .iter()
        .map(|k| yaml_str(&compose, &format!("secrets.{k}.environment")))
        .collect();
    assert_eq!(read_from, SECRET_INPUTS, "each input a secret read from the environment");
    for input in SECRET_INPUTS {
        assert!(!compose_text.contains(&format!("${{{input}")), "{input} is interpolated: {compose_text}");
        assert!(
            delivery.contains(&format!("\"ISOLOOM_SECRET_INPUT_{input}\" => isoloom_b64(INPUTS[\"{input}\"])")),
            "{input}: {delivery}"
        );
        assert!(
            !vagrantfile.every_statement().iter().any(|s| s.contains(&format!("\"{input}\" => isoloom_b64"))),
            "{input} is a plain input"
        );
    }
    Ok(())
}
