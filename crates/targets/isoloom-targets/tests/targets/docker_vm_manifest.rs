//! `docker-vm`'s two run manifests: the VM made by Vagrant (`docker-vm/run.json`) or by Terraform
//! on Proxmox (`docker-vm/proxmox/run.json`), then the same Compose commands run in it through
//! each one's way in, the values given at launch carried into the VM and accepted there, never
//! written in a generated file.

use isoloom_manifest::{Action, Condition, Delivery, HostArgv, Manifest, Run, StepSelection};
use isoloom_spec::Target;
use isoloom_targets::{GeneratedFile, generate};

use crate::testing::{Outcome, file, lab};

/// A web container with a flag file and a published port, and a set-up job nothing waits for.
const FLAGGED: &str = "
version: 1
name: flagged
inputs: [FLAG]
networks: { lab: { cidr: 10.31.0.0/24, internet: false } }
machines:
  web:
    networks: { lab: 10 }
    ports: { site: { port: 80, protocol: http, publish: 8080 } }
    files: [{ path: /ctf/flag, input: FLAG }]
    app: { image: nginx }
    setup: [seed.sh]
checks:
  - { http: \"http://web/\" }
";

fn manifests() -> Outcome<(Vec<GeneratedFile>, Manifest, Manifest)> {
    let files = generate(&lab(FLAGGED)?, Target::DockerVm)?;
    let local = isoloom_manifest::parse(file(&files, ".isoloom/docker-vm/run.json")?)?;
    let server = isoloom_manifest::parse(file(&files, ".isoloom/docker-vm/proxmox/run.json")?)?;
    Ok((files, local, server))
}

/// The ids of a manifest's `up` steps.
fn up(m: &Manifest) -> Vec<&str> {
    m.lifecycle.up.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn vagrant_brings_the_vm_up_then_compose_starts_in_it_over_vagrant_ssh() -> Outcome {
    let (_, m, _) = manifests()?;
    assert_eq!(up(&m), ["reconcile", "discard-saved", "forwarded-ports", "up", "start", "job-web-init-1"]);
    let vm = m.lifecycle.up.iter().find(|s| s.id.as_str() == "up").unwrap();
    let Run::Argv(HostArgv::Every(argv)) = &vm.run else { panic!() };
    let up_args: Vec<String> = argv.args().iter().map(ToString::to_string).collect();
    // Every provisioner but the lab's start: Compose is the next steps'.
    assert_eq!(up_args[up_args.len() - 2..], ["--provision-with", "docker,accept-env,project"]);
    let start = m.lifecycle.up.iter().find(|s| s.id.as_str() == "start").unwrap();
    let Run::Argv(HostArgv::Every(start_argv)) = &start.run else { panic!() };
    let piped: Vec<String> = start_argv.args().iter().map(ToString::to_string).collect();
    assert_eq!(piped[..3], ["ssh", "--no-tty", "--command"]);
    assert!(
        piped[3].starts_with("cd /opt/isoloom && sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose --project-name flagged"),
        "{}",
        piped[3]
    );
    assert_eq!(piped[4], "default");
    // A forwarded port is checked free first, except where nothing is forwarded (ESXi).
    let ports = m.lifecycle.up.iter().find(|s| s.id.as_str() == "forwarded-ports").unwrap();
    assert!(matches!(
        ports.run,
        Run::Action {
            action: Action::VagrantCheckForwardedPorts,
            ..
        }
    ));
    assert!(!ports.providers.contains(&isoloom_manifest::Provider::VmwareEsxi));
    // Parked by suspending the VM only; Hyper-V isn't one of the providers.
    let park = m.lifecycle.park.as_ref().unwrap();
    assert!(park.pause.is_some() && park.shutdown.is_none());
    let Some(isoloom_manifest::Identity::Vagrant(v)) = Some(&m.identity) else {
        panic!()
    };
    assert!(v.providers.iter().all(|p| p.id != isoloom_manifest::Provider::Hyperv));
    Ok(())
}

#[test]
fn proxmox_makes_the_vm_then_sets_it_up_copies_the_project_and_starts_compose_over_ssh() -> Outcome {
    let (_, _, m) = manifests()?;
    assert_eq!(
        up(&m),
        ["clear-lock", "init", "apply", "docker", "archive", "project", "start", "job-web-init-1"]
    );
    let step = |id: &str| m.lifecycle.up.iter().find(|s| s.id.as_str() == id).unwrap();
    assert_eq!(step("docker").stdin.as_ref().map(ToString::to_string).as_deref(), Some("setup.sh"));
    let Run::Action {
        action: Action::ProjectArchive,
        with,
    } = &step("archive").run
    else {
        panic!()
    };
    assert_eq!(with.values().map(ToString::to_string).collect::<Vec<_>>(), ["../../.."]);
    assert_eq!(
        step("project").stdin.as_ref().map(ToString::to_string).as_deref(),
        Some("${run.state_dir}/project.tgz")
    );
    for id in ["docker", "project", "start", "job-web-init-1"] {
        let Run::Action {
            action: Action::SshRun,
            with: args,
        } = &step(id).run
        else {
            panic!("{id}")
        };
        let host = args.iter().find(|(k, _)| k.as_str() == "host").map(|(_, v)| v.to_string());
        assert_eq!(host.as_deref(), Some("${outputs.ip}"), "{id}");
    }
    // No park on a server; the containers answer at the VM's address.
    assert!(m.lifecycle.park.is_none());
    assert_eq!(
        m.guest.as_ref().and_then(|g| g.host.as_ref()).map(ToString::to_string).as_deref(),
        Some("${outputs.ip}")
    );
    let ready: Vec<String> = m
        .ready
        .iter()
        .filter_map(|c| match c {
            Condition::StepOk { step: s, .. } => Some(s.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(ready, ["apply", "start", "job-web-init-1"]);
    Ok(())
}

#[test]
fn the_flag_reaches_the_start_steps_and_is_accepted_in_the_vm_never_written() -> Outcome {
    let (files, local, server) = manifests()?;
    for m in [&local, &server] {
        let flag = m.secrets.iter().find(|s| s.name.as_str() == "FLAG").unwrap();
        let Some(Delivery::Env {
            var,
            steps: StepSelection::Only(steps),
        }) = flag.deliver.iter().next()
        else {
            panic!()
        };
        assert_eq!(var.as_str(), "FLAG");
        assert_eq!(
            steps.iter().map(isoloom_manifest::StepId::as_str).collect::<Vec<_>>(),
            ["start", "job-web-init-1"]
        );
        let guest = m.guest.as_ref().unwrap();
        assert_eq!(guest.machine.as_str(), "docker");
        assert!(m.machines.iter().any(|x| x.name.as_str() == "docker"));
    }
    let vagrantfile = file(&files, ".isoloom/docker-vm/Vagrantfile")?;
    assert!(vagrantfile.contains("config.ssh.forward_env = [\"FLAG\"]"), "{vagrantfile}");
    assert!(
        vagrantfile.contains("echo 'AcceptEnv FLAG' > /etc/ssh/sshd_config.d/isoloom-env.conf"),
        "{vagrantfile}"
    );
    let main = file(&files, ".isoloom/docker-vm/proxmox/main.tf")?;
    assert!(main.contains("AcceptEnv FLAG"), "{main}");
    // Terraform only makes the VM: nothing of it reaches the guest.
    assert!(
        !main.contains("terraform_data") && !main.contains("local-exec") && !main.contains("remote-exec"),
        "{main}"
    );
    let setup = file(&files, ".isoloom/docker-vm/proxmox/setup.sh")?;
    assert!(setup.starts_with("#!/bin/sh\n") && setup.contains("get.docker.com"), "{setup}");
    Ok(())
}

#[test]
fn the_containers_published_ports_answer_where_the_driver_says_the_vm_does() -> Outcome {
    let (_, local, server) = manifests()?;
    for m in [&local, &server] {
        let site = m.outputs.iter().find(|o| o.name.as_str() == "web_site").unwrap();
        assert_eq!(site.format.as_ref().map(ToString::to_string).as_deref(), Some("http://${host.web}:${value}/"));
        let web = m.machines.iter().find(|x| x.name.as_str() == "web").unwrap();
        let publish = web.ports[0].publish.as_ref().unwrap();
        assert_eq!(
            (publish.mode, publish.preferred.map(u16::from)),
            (isoloom_manifest::PublishMode::Fixed, Some(8080))
        );
    }
    Ok(())
}
