//! The run manifest the `vagrant` target writes (`.isoloom/vagrant/run.json`): two specs shaped
//! like the corpus's VM environments write the shared fixtures the runner tests against (a Windows domain
//! with a controller, and a Linux web machine with its secret as a file), check runners get the
//! docker target's time rule, and an input left out is delivered empty. Every example's manifest
//! is checked in `run_manifest`, with the docker target's.

use isoloom_manifest::{Action, Delivery, Identity, Manifest, Provider, Role, Run, SetBy, StepSelection};
use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, file, lab, matches_fixture};

/// A small Active Directory environment, `mini-ad`: two Windows machines (the secret a file on `dc01`), a
/// domain built by the environment's playbook from the controller, checks with long waits.
const MINI_AD: &str = "
version: 1
name: mini-ad
networks:
  lab: { cidr: 10.50.0.0/24 }
inputs: [DEMO_SECRET]
machines:
  dc01:
    os: windows-server@2019
    networks: { lab: 10 }
    ports: { ldap: 389, smb: 445 }
    resources: { cpus: 2, memory_mb: 4096 }
    files: [{ path: C:/secret.txt, input: DEMO_SECRET }]
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

/// A Linux web machine whose secret is a file, its page published on the host, checked from the
/// networks.
const WEB_VM: &str = "
version: 1
name: web-vm
networks:
  lab: { cidr: 10.61.0.0/24 }
inputs: [DEMO_SECRET]
machines:
  web:
    os: debian@12
    networks: { lab: 10 }
    ports: { web: { port: 80, protocol: http, publish: 8086 } }
    resources: { cpus: 1, memory_mb: 768 }
    files: [{ path: /srv/secret.txt, input: DEMO_SECRET }]
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

#[test]
fn the_mini_ad_spec_writes_the_shared_vagrant_fixture() -> Outcome {
    matches_fixture(manifest(MINI_AD)?, "vagrant.run.json")
}

#[test]
fn the_web_vm_spec_writes_the_shared_linux_vagrant_fixture() -> Outcome {
    matches_fixture(manifest(WEB_VM)?, "vagrant-linux.run.json")
}

#[test]
fn the_controller_is_infra_halted_after_up_but_not_on_qemu() -> Outcome {
    let m = manifest(MINI_AD)?;
    let controller = m.machines.iter().find(|x| x.name.as_str() == "isoloom-controller").unwrap();
    assert_eq!((controller.role, controller.keep_running), (Role::Infra, false));
    let park = m.lifecycle.up.iter().find(|s| s.id.as_str() == "park-controller").unwrap();
    assert!(!park.providers.contains(&Provider::Qemu) && park.providers.contains(&Provider::Virtualbox));
    // An action, not `vagrant halt`: after `up` the controller powers itself off, and a halt
    // racing that fails over SSH (libvirt).
    assert!(matches!(
        park.run,
        Run::Action {
            action: Action::VagrantParkController,
            ..
        }
    ));
    // The play's failure leads the error, and a resume never boots the controller.
    let up = m.lifecycle.up.iter().find(|s| s.id.as_str() == "up").unwrap();
    assert!(up.verdict.as_deref().is_some_and(|v| v.starts_with("fatal:")));
    let resume = serde_json::to_string(&m.lifecycle.resume).unwrap();
    assert!(resume.contains("\"dc01\",\"ws01\"") && !resume.contains("isoloom-controller"), "{resume}");
    // A pause suspends what runs, not the halted controller (Hyper-V refuses to suspend a VM
    // that is off).
    let pause = serde_json::to_string(&m.lifecycle.park.as_ref().unwrap().pause).unwrap();
    assert!(
        pause.contains("\"suspend\",\"dc01\",\"ws01\"") && !pause.contains("isoloom-controller"),
        "{pause}"
    );
    Ok(())
}

#[test]
fn a_saved_state_that_cant_be_restored_is_discarded_and_the_vms_boot() -> Outcome {
    // VirtualBox 7.2 refuses its own VMSVGA saved states (`Failed to load unit 'vga'`).
    let m = manifest(WEB_VM)?;
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
    let Identity::Vagrant(v) = manifest(WEB_VM)?.identity else {
        panic!("a vagrant identity")
    };
    assert_eq!(v.dotfile.unwrap().template().to_string(), "${run.state_dir}/vagrant");
    Ok(())
}

#[test]
fn the_values_given_at_launch_reach_the_steps_that_run_provisioners() -> Outcome {
    let m = manifest(MINI_AD)?;
    let secret = m.secrets.iter().find(|s| s.name.to_string() == "DEMO_SECRET").unwrap();
    assert_eq!(secret.machines.iter().map(ToString::to_string).collect::<Vec<_>>(), ["dc01"]);
    let Delivery::Env { steps, .. } = &secret.deliver.as_slice()[0] else {
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
    let manifests = [manifest(MINI_AD)?, manifest(WEB_VM)?];
    let inputs: Vec<(bool, Option<String>)> = manifests
        .iter()
        .flat_map(|m| m.secrets.iter().filter(|s| s.name.to_string() == "DEMO_SECRET"))
        .map(|s| (s.required, s.default.clone()))
        .collect();
    assert_eq!(inputs, [(false, Some(String::new())), (false, Some(String::new()))]);
    Ok(())
}

#[test]
fn the_message_leaves_the_forwarded_port_to_the_runner() -> Outcome {
    let m = manifest(WEB_VM)?;
    assert_eq!(
        m.environment.message.unwrap().to_string(),
        "The page answers at http://localhost:${published.web.web}/."
    );
    Ok(())
}

/// Vagrant picks each box's build for the computer it runs on (`box_architecture`'s `:auto`),
/// but ESXi's VMs run on the ESXi host, x86-64: from an Apple Silicon Mac, Vagrant took
/// `bento/debian-12`'s arm64 build and uploaded it to an x86-64 ESXi, which `ovftool` refused.
/// On ESXi, and there only, the runner gives every step Vagrant's `VAGRANT_HOST_ARCHITECTURE`
/// as `amd64`: Vagrant then takes a box's amd64 build, or the one build of a box that names no
/// architecture, as on an x86-64 computer.
#[test]
fn on_esxi_vagrant_picks_the_boxes_builds_for_the_esxi_hosts_amd64() -> Outcome {
    let containers = "version: 1\nname: c\nnetworks:\n  lab: { cidr: 10.62.0.0/24 }\nmachines:\n  web: { networks: { lab: 10 }, app: { image: nginx } }\n";
    let docker_vm = generate(&lab(containers)?, Target::DockerVm)?;
    let docker_vm = isoloom_manifest::parse(file(&docker_vm, ".isoloom/docker-vm/run.json")?)?;
    for m in [manifest(WEB_VM)?, manifest(MINI_AD)?, docker_vm] {
        let arch = m
            .params
            .iter()
            .find(|p| p.name.as_str() == "VAGRANT_HOST_ARCHITECTURE")
            .ok_or("no VAGRANT_HOST_ARCHITECTURE param")?;
        assert_eq!(arch.providers.iter().copied().collect::<Vec<_>>(), [Provider::VmwareEsxi]);
        assert!(arch.hosts.is_empty());
        assert_eq!((arch.default.as_deref(), arch.values.as_slice()), (Some("amd64"), &["amd64".to_owned()][..]));
        assert_eq!(arch.set_by, SetBy::Runner);
        let [Delivery::Env { var, steps }] = arch.deliver.as_slice() else {
            panic!("one environment variable: {:?}", arch.deliver)
        };
        assert_eq!((var.as_str(), steps), ("VAGRANT_HOST_ARCHITECTURE", &StepSelection::Every));
    }
    Ok(())
}

/// `vagrant status` on ESXi asks the host once per VM (41 s for two VMs, measured on
/// `esxi-lab`): there the read gets 60 s and 30 s a VM; the local providers keep 25 s.
#[test]
fn on_esxi_the_status_read_gets_time_for_each_vm() -> Outcome {
    for (spec, vms) in [(WEB_VM, 2), (MINI_AD, 3)] {
        let m = manifest(spec)?;
        assert_eq!(m.machines.len(), vms);
        let status = &m.lifecycle.status;
        assert_eq!(status.timeout_s.map(std::num::NonZeroU32::get), Some(25));
        let on: Vec<(Provider, u32)> = status.provider_timeout_s.iter().map(|(p, t)| (*p, t.get())).collect();
        assert_eq!(on, [(Provider::VmwareEsxi, 60 + 30 * u32::try_from(vms)?)]);
    }
    Ok(())
}

/// ESXi forwards no port to this computer: a published port is shown at the host the driver
/// reports for its machine (the loopback with the forwarded port locally, the VM's own address
/// with its own port on ESXi).
#[test]
fn a_published_port_is_shown_where_the_driver_says_it_answers() -> Outcome {
    let m = manifest(WEB_VM)?;
    let formats: Vec<String> = m
        .outputs
        .iter()
        .map(|o| o.format.as_ref().map(ToString::to_string).unwrap_or_default())
        .collect();
    assert_eq!(formats, ["http://${host.web}:${value}/"]);
    Ok(())
}
