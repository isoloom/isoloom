//! The run manifest the `vagrant` target writes (`.isoloom/vagrant/run.json`): two specs shaped
//! like the corpus's VM labs write the shared fixtures the runner tests against (a Windows domain
//! with a controller, and a Linux web machine with its flag as a file), check runners get the
//! docker target's time rule, and an input left out is delivered empty. Every example's manifest
//! is checked in `run_manifest`, with the docker target's.

use std::path::{Path, PathBuf};

use isoloom_manifest::{Delivery, Identity, Manifest, Provider, Role, Version};
use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, file, lab};

/// run-manifest-v0.md §10's `mini-ad`: two Windows machines (the flag a file on `dc01`), a
/// domain built by the environment's playbook from the controller, checks with long waits.
const MINI_AD: &str = "
version: 1
name: mini-ad
networks:
  lab: { cidr: 10.50.0.0/24 }
inputs: [CTF_FLAG_MAIN]
machines:
  dc01:
    os: windows-server@2019
    networks: { lab: 10 }
    ports: { ldap: 389, smb: 445 }
    resources: { cpus: 2, memory_mb: 4096 }
    files: [{ path: C:/flag.txt, input: CTF_FLAG_MAIN }]
  ws01:
    os: windows@10
    networks: { lab: 20 }
    depends_on: [dc01]
    resources: { cpus: 2, memory_mb: 4096 }
provision:
  - ansible: provision/domain.yml
    groups: { dcs: [dc01], workstations: [ws01] }
checks:
  - { tcp: \"dc01:389\", wait: 300 }
  - { script: checks/kerberoast.sh, wait: 600 }
";

/// A Linux web machine whose flag is a file, its page published on the host, checked from the
/// networks.
const FLAG_VM: &str = "
version: 1
name: flag-vm
networks:
  lab: { cidr: 10.61.0.0/24 }
inputs: [CTF_FLAG_MAIN]
machines:
  web:
    os: debian@12
    networks: { lab: 10 }
    ports: { web: { port: 80, protocol: http, publish: 8086 } }
    resources: { cpus: 1, memory_mb: 768 }
    files: [{ path: /srv/flag.txt, input: CTF_FLAG_MAIN }]
    app: { steps: [{ run: provision/web.sh }] }
checks:
  - { http: \"http://web/\", wait: 120 }
message: \"The page answers at http://localhost:{{ machines.web.ports.web.published }}/.\"
";

/// Where the vagrant target writes its run manifest.
const RUN_JSON: &str = ".isoloom/vagrant/run.json";

/// The vagrant run manifest of a spec written in a test.
fn manifest(yaml: &str) -> Outcome<Manifest> {
    let files = generate(&lab(yaml)?, Target::Vagrant)?;
    Ok(isoloom_manifest::parse(file(&files, RUN_JSON)?)?)
}

/// The shared run-manifest fixture `name`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/isoloom-manifest/fixtures").join(name)
}

/// `generated` against the fixture `name`; `ISOLOOM_WRITE_FIXTURES=1` writes it instead.
fn matches_fixture(mut generated: Manifest, name: &str) -> Outcome {
    let path = fixture(name);
    if std::env::var_os("ISOLOOM_WRITE_FIXTURES").is_some() {
        std::fs::write(&path, isoloom_manifest::to_json(&generated)?)?;
    }
    let mut fixture = isoloom_manifest::parse(&std::fs::read_to_string(&path)?)?;
    // A release changes the version, not the contract.
    generated.generator.version = Version::parse("0.0.0")?;
    fixture.generator.version = Version::parse("0.0.0")?;
    assert_eq!(generated, fixture, "{name}");
    Ok(())
}

#[test]
fn the_mini_ad_spec_writes_the_shared_vagrant_fixture() -> Outcome {
    matches_fixture(manifest(MINI_AD)?, "vagrant.run.json")
}

#[test]
fn the_flag_vm_spec_writes_the_shared_linux_vagrant_fixture() -> Outcome {
    matches_fixture(manifest(FLAG_VM)?, "vagrant-linux.run.json")
}

#[test]
fn the_controller_is_infra_halted_after_up_but_not_on_qemu() -> Outcome {
    let m = manifest(MINI_AD)?;
    let controller = m.machines.iter().find(|x| x.name.as_str() == "isoloom-controller").unwrap();
    assert_eq!((controller.role, controller.keep_running), (Role::Infra, false));
    let park = m.lifecycle.up.iter().find(|s| s.id.as_str() == "park-controller").unwrap();
    assert!(!park.providers.contains(&Provider::Qemu) && park.providers.contains(&Provider::Virtualbox));
    // The play's failure leads the error, and a resume never boots the controller.
    let up = m.lifecycle.up.iter().find(|s| s.id.as_str() == "up").unwrap();
    assert!(up.verdict.as_deref().is_some_and(|v| v.starts_with("fatal:")));
    let resume = serde_json::to_string(&m.lifecycle.resume).unwrap();
    assert!(resume.contains("\"dc01\",\"ws01\"") && !resume.contains("isoloom-controller"), "{resume}");
    Ok(())
}

#[test]
fn a_saved_state_that_cant_be_restored_is_discarded_and_the_vms_boot() -> Outcome {
    // VirtualBox 7.2 refuses its own VMSVGA saved states (`Failed to load unit 'vga'`).
    let m = manifest(FLAG_VM)?;
    let ids: Vec<&str> = m.lifecycle.resume.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["discard-saved", "resume", "discard-cut-off", "boot"]);
    let resume = &m.lifecycle.resume[1];
    assert_eq!(resume.failure, isoloom_manifest::Failure::Tolerate);
    assert!(m.lifecycle.resume[2].providers.contains(&Provider::Virtualbox));
    assert_eq!(m.lifecycle.resume[3].failure, isoloom_manifest::Failure::Fail);
    Ok(())
}

#[test]
fn check_runners_get_their_waits_plus_15_s_a_check_plus_60_s() -> Outcome {
    let m = manifest(MINI_AD)?;
    let runner = &m.checks.runners[0];
    let waits: u32 = runner.checks.iter().map(|c| c.wait_s).sum();
    let n = u32::try_from(runner.checks.len()).unwrap();
    assert_eq!(runner.step.timeout_s.unwrap().get(), waits + 15 * n + 60);
    assert_eq!(m.checks.setup[0].id.as_str(), "wake-controller", "the controller's boot isn't check time");
    Ok(())
}

#[test]
fn vagrants_state_lives_in_the_runs_state_folder() -> Outcome {
    let Identity::Vagrant(v) = manifest(FLAG_VM)?.identity else {
        panic!("a vagrant identity")
    };
    assert_eq!(v.dotfile.unwrap().template().to_string(), "${run.state_dir}/vagrant");
    Ok(())
}

#[test]
fn the_values_given_at_launch_reach_the_steps_that_run_provisioners() -> Outcome {
    let m = manifest(MINI_AD)?;
    let flag = m.secrets.iter().find(|s| s.name.to_string() == "CTF_FLAG_MAIN").unwrap();
    assert_eq!(flag.machines.iter().map(ToString::to_string).collect::<Vec<_>>(), ["dc01"]);
    let Delivery::Env { steps, .. } = &flag.deliver.as_slice()[0] else {
        panic!("env")
    };
    assert_eq!(
        serde_json::to_value(steps).unwrap(),
        serde_json::json!(["up", "provision", "provision-machine"])
    );
    let password = m.secrets.iter().find(|s| s.name.to_string() == "ESXI_PASSWORD").unwrap();
    assert!(password.required && password.providers.contains(&Provider::VmwareEsxi));
    Ok(())
}

/// An input left out is delivered empty (every example's slots are checked with the docker
/// target's, in `run_manifest`).
#[test]
fn an_input_left_out_is_delivered_empty() -> Outcome {
    let manifests = [manifest(MINI_AD)?, manifest(FLAG_VM)?];
    let inputs: Vec<(bool, Option<String>)> = manifests
        .iter()
        .flat_map(|m| m.secrets.iter().filter(|s| s.name.to_string() == "CTF_FLAG_MAIN"))
        .map(|s| (s.required, s.default.clone()))
        .collect();
    assert_eq!(inputs, [(false, Some(String::new())), (false, Some(String::new()))]);
    Ok(())
}

#[test]
fn the_message_leaves_the_forwarded_port_to_the_runner() -> Outcome {
    let m = manifest(FLAG_VM)?;
    assert_eq!(
        m.lab.message.unwrap().to_string(),
        "The page answers at http://localhost:${published.web.web}/."
    );
    Ok(())
}
