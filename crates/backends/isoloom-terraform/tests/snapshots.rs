//! The AWS, Azure, Google Cloud and Proxmox modules of a few examples, rendered from their plan, as snapshots; their
//! determinism, and refusals read from the plan without rendering.

pub mod testing;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmBackend, CloudVmModule, ProxmoxBackend};
use testing::{Outcome, example, file, planned, spec};

/// Example `name`'s plan for `target`.
fn env(name: &str, target: Target) -> Outcome<Environment> {
    Ok(planned(&example(name)?, target))
}

/// The module at `suffix` that `backend` renders for example `name`.
fn module(backend: &dyn Backend, name: &str, suffix: &str) -> Outcome<String> {
    let files = backend.render(&env(name, backend.target())?)?;
    Ok(file(&files, suffix)?.to_owned())
}

fn aws(name: &str) -> Outcome<String> {
    module(&CloudVmBackend, name, "cloud-vm/aws/main.tf")
}

fn azure(name: &str) -> Outcome<String> {
    module(&CloudVmBackend, name, "cloud-vm/azure/main.tf")
}

fn gcp(name: &str) -> Outcome<String> {
    module(&CloudVmBackend, name, "cloud-vm/gcp/main.tf")
}

fn proxmox(name: &str) -> Outcome<String> {
    module(&ProxmoxBackend, name, "proxmox/main.tf")
}

#[test]
fn aws_hello_stack() -> Outcome {
    insta::assert_snapshot!(aws("hello-stack")?);
    Ok(())
}

#[test]
fn aws_segmented() -> Outcome {
    insta::assert_snapshot!(aws("segmented")?);
    Ok(())
}

#[test]
fn aws_windows_hello() -> Outcome {
    insta::assert_snapshot!(aws("windows-hello")?);
    Ok(())
}

#[test]
fn aws_ansible_pair() -> Outcome {
    insta::assert_snapshot!(aws("ansible-pair")?);
    Ok(())
}

#[test]
fn azure_hello_stack() -> Outcome {
    insta::assert_snapshot!(azure("hello-stack")?);
    Ok(())
}

#[test]
fn azure_segmented() -> Outcome {
    insta::assert_snapshot!(azure("segmented")?);
    Ok(())
}

#[test]
fn azure_windows_hello() -> Outcome {
    insta::assert_snapshot!(azure("windows-hello")?);
    Ok(())
}

#[test]
fn gcp_solo_web() -> Outcome {
    insta::assert_snapshot!(gcp("solo-web")?);
    Ok(())
}

#[test]
fn gcp_hello_stack() -> Outcome {
    insta::assert_snapshot!(gcp("hello-stack")?);
    Ok(())
}

#[test]
fn proxmox_hello_stack() -> Outcome {
    insta::assert_snapshot!(proxmox("hello-stack")?);
    Ok(())
}

#[test]
fn proxmox_segmented() -> Outcome {
    insta::assert_snapshot!(proxmox("segmented")?);
    Ok(())
}

#[test]
fn proxmox_ansible_pair() -> Outcome {
    insta::assert_snapshot!(proxmox("ansible-pair")?);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "segmented", "vlan-office", "existing-hosts", "windows-hello"] {
        let e = env(name, Target::CloudVm)?;
        assert_eq!(CloudVmBackend.render(&e), CloudVmBackend.render(&e), "{name}");
    }
    for name in ["hello-stack", "ansible-pair", "existing-hosts"] {
        let e = env(name, Target::Proxmox)?;
        assert_eq!(ProxmoxBackend.render(&e), ProxmoxBackend.render(&e), "{name}");
    }
    Ok(())
}

/// One arm64 machine: Proxmox runs it (an emulated aarch64 VM); every cloud refuses it.
const ARM64: &str = "version: 1\nname: r\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w:\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    os: debian@12\n";

#[test]
fn refusals_come_from_the_plan_and_match_rendering() -> Outcome {
    let why = "no cloud can run it (aws: machine `w`: arm64 on AWS comes later (Graviton instances and arm64 AMIs); \
         azure: machine `w`: arm64 on Azure comes later (Arm-based sizes and arm64 images); \
         gcp: machine `w`: arm64 on Google Cloud comes later (Arm-based machine types and arm64 images); \
         digitalocean: machine `w`: arm64 on DigitalOcean comes later (it has no arm64 droplets); \
         linode: machine `w`: arm64 on Linode comes later (it has no arm64 plans); \
         oci: machine `w`: arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images))";
    let e = planned(&spec(ARM64)?, CloudVmBackend.target());
    assert_eq!(CloudVmBackend.refusal(&e).as_deref(), Some(why));
    let err = CloudVmBackend.render(&e).expect_err("refused");
    assert!(err.to_string().ends_with(why), "{err}");
    Ok(())
}

#[test]
fn proxmox_windows_hello() -> Outcome {
    // Not run on a real host: a clone of a Windows template, set up by cloudbase-init.
    insta::assert_snapshot!(proxmox("windows-hello")?);
    Ok(())
}

/// A spec of one machine, `w`, with `fields` (its OS, steps and hardware).
fn one_vm(fields: &str) -> Outcome<isoloom_plan::Lab> {
    spec(&format!(
        "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w: {{ networks: {{ lab: 10 }}, {fields} }}\n"
    ))
}

#[test]
fn a_tpm_is_given_on_proxmox_and_the_clouds_and_refused_where_none_is() -> Outcome {
    let tpm = one_vm("os: debian@12, app: { steps: [{ run: a.sh }] }, hardware: { tpm: required }")?;
    let proxmox = ProxmoxBackend.render(&planned(&tpm, Target::Proxmox))?;
    let proxmox = file(&proxmox, "proxmox/main.tf")?;
    assert!(proxmox.contains("tpm_state {"), "{proxmox}");
    let cloud = planned(&tpm, Target::CloudVm);
    let clouds = CloudVmBackend.render(&cloud)?;
    assert!(file(&clouds, "cloud-vm/aws/main.tf")?.contains("tpm_support == \"v2.0\""));
    assert!(file(&clouds, "cloud-vm/azure/main.tf")?.contains("vtpm_enabled"));
    assert!(file(&clouds, "cloud-vm/gcp/main.tf")?.contains("enable_vtpm"));
    assert!(file(&clouds, "cloud-vm/oci/main.tf")?.contains("is_trusted_platform_module_enabled"));
    let digitalocean = CloudVmModule::new(Cloud::DigitalOcean).refusal(&cloud).expect("refused");
    assert!(
        digitalocean.contains("asks for a virtual TPM (`hardware.tpm: required`): DigitalOcean gives none"),
        "{digitalocean}"
    );
    // Windows with a TPM: AWS refuses it, which drops only its module; Azure's trusted launch is
    // written.
    let windows = planned(
        &one_vm("os: windows-server@2022, app: { steps: [{ run: a.ps1 }] }, hardware: { tpm: required }")?,
        Target::CloudVm,
    );
    let aws = CloudVmModule::new(Cloud::Aws).refusal(&windows).expect("refused");
    assert!(aws.contains("NitroTPM"), "{aws}");
    assert_eq!(CloudVmBackend.refusal(&windows), None);
    let rendered = CloudVmBackend.render(&windows)?;
    assert!(!rendered.iter().any(|f| f.path.contains("cloud-vm/aws/")));
    let azure = file(&rendered, "cloud-vm/azure/main.tf")?;
    assert!(azure.contains("vtpm_enabled"), "{azure}");
    // Windows 11 merely expects one: the clouds leave it out instead of refusing.
    let wanted = planned(&one_vm("os: windows@11, app: { steps: [{ run: a.ps1 }] }")?, Target::CloudVm);
    assert_eq!(wanted.machines["w"].tpm, Some(isoloom_spec::windows::Tpm::Wanted));
    assert!(isoloom_plan::refusal(&wanted.requirements, &CloudVmBackend.capabilities()).is_none());
    Ok(())
}

#[test]
fn proxmox_refuses_what_its_windows_machines_cant_have_yet() -> Outcome {
    // Validation refuses a playbook step on Windows; the backend still guards its own plan.
    let yaml = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, os: windows-server@2022, app: { steps: [{ playbook: a.yml }] } }\n";
    let why = testing::refusal(&ProxmoxBackend, yaml)?.expect("refused");
    assert!(why.starts_with("machine `w`: Windows provisioning steps are PowerShell scripts"), "{why}");
    Ok(())
}

#[test]
fn each_cloud_refuses_on_its_own() -> Outcome {
    // Two networks, each machine on one: AWS, Azure and Google Cloud take it.
    let refusals = CloudVmBackend.refusals(&env("segmented", Target::CloudVm)?);
    let clouds: Vec<Cloud> = refusals.iter().map(|(c, _)| *c).collect();
    assert_eq!(clouds, Cloud::ALL);
    assert!(refusals[..3].iter().all(|(_, why)| why.is_none()), "{refusals:?}");
    // A Windows machine: Google Cloud only.
    let windows = env("windows-hello", Target::CloudVm)?;
    let on_windows = CloudVmBackend.refusals(&windows);
    assert_eq!(on_windows[2].1.as_deref(), Some("machine `web01`: Windows on GCP comes later"));
    let files = CloudVmBackend.render(&windows)?;
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/azure/main.tf")));
    assert!(!files.iter().any(|f| f.path.ends_with("cloud-vm/gcp/main.tf")));
    Ok(())
}

#[test]
fn a_cloud_refuses_from_the_plan_without_rendering() -> Outcome {
    let e = planned(&spec(ARM64)?, Target::CloudVm);
    for (cloud, why) in [
        (Cloud::Azure, "machine `w`: arm64 on Azure comes later (Arm-based sizes and arm64 images)"),
        (
            Cloud::Gcp,
            "machine `w`: arm64 on Google Cloud comes later (Arm-based machine types and arm64 images)",
        ),
    ] {
        let module = CloudVmModule::new(cloud);
        assert_eq!(module.refusal(&e).as_deref(), Some(why));
        assert!(module.render(&e).unwrap_err().to_string().ends_with(why));
    }
    Ok(())
}
