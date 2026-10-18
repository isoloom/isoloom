//! GPUs, host devices and nested virtualization on every target: rendered where the target can
//! give them, refused with the reason elsewhere.

use isoloom_spec::Target;
use isoloom_targets::{Error, GenerateError, Lab, generate};

use crate::testing::{Outcome, generated as file, lab};

/// One machine with `hardware` set, as a container (an image) and as a VM (its OS and a step).
fn spec(hardware: &str) -> Outcome<Lab> {
    lab(&format!(
        "version: 1\nname: hw\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ os: debian@12, networks: {{ lab: 10 }}, app: {{ image: nginx, steps: [{{ run: a.sh }}] }}, hardware: {{ {hardware} }} }}\n"
    ))
}

/// One machine with `hardware` set, its OS and a step (a VM on the VM targets).
fn vm(hardware: &str) -> Outcome<Lab> {
    lab(&format!(
        "version: 1\nname: hw\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ os: debian@12, networks: {{ lab: 10 }}, app: {{ steps: [{{ run: a.sh }}] }}, hardware: {{ {hardware} }} }}\n"
    ))
}

/// Why `target` refuses the lab.
fn refusal(spec: &Lab, target: Target) -> Outcome<String> {
    match generate(spec, target) {
        Err(Error::Refused(GenerateError::Unsupported { what, .. })) => Ok(what),
        other => Err(format!("{target:?} doesn't refuse it: {:?}", other.map(|f| f.len())).into()),
    }
}

#[test]
fn a_container_gets_the_hosts_gpus_and_devices_on_docker_only() -> Outcome {
    let s = spec("gpus: { count: 2, capabilities: [compute] }, devices: ['/dev/fuse:/dev/fuse:rw']")?;
    let compose = file(&s, Target::Docker, "docker/compose.yml")?;
    assert!(
        compose.contains("    devices:\n    - source: /dev/fuse\n      target: /dev/fuse\n      permissions: rw\n"),
        "{compose}"
    );
    assert!(
        compose.contains("          - capabilities:\n            - gpu\n            - compute\n            driver: nvidia\n            count: 2\n"),
        "{compose}"
    );
    for target in [Target::DockerVm, Target::CloudDocker] {
        assert!(
            refusal(&s, target)?.contains("asks for GPUs (`gpus`): its container runs on a VM Isoloom creates"),
            "{target:?}"
        );
    }
    let devices_only = spec("devices: [/dev/fuse]")?;
    assert!(refusal(&devices_only, Target::Kubernetes)?.contains("is given host devices (`devices`)"));
    Ok(())
}

#[test]
fn kubernetes_asks_the_device_plugin_for_a_count() -> Outcome {
    let s = spec("gpus: { count: 1, capabilities: [compute, utility] }")?;
    let manifests = file(&s, Target::Kubernetes, "kubernetes/environment.yaml")?;
    assert!(manifests.contains("nvidia.com/gpu: '1'"), "{manifests}");
    assert!(manifests.contains("- name: NVIDIA_DRIVER_CAPABILITIES\n"), "{manifests}");
    assert!(refusal(&spec("gpus: all")?, Target::Kubernetes)?.contains("`gpus: all` has no Kubernetes form"));
    Ok(())
}

/// Each cloud's module of `cloud-vm` for `spec`, by its folder (`None`: that cloud refuses).
fn cloud_module(spec: &Lab, cloud: &str) -> Outcome<Option<String>> {
    let files = generate(spec, Target::CloudVm)?;
    Ok(files.into_iter().find(|f| f.path.contains(&format!("/{cloud}/main.tf"))).map(|f| f.contents))
}

#[test]
fn vms_nest_on_vagrant_proxmox_and_three_clouds() -> Outcome {
    let nested = vm("nested_virtualization: true")?;
    let v = file(&nested, Target::Vagrant, "vagrant/Vagrantfile")?;
    for switch in [
        "\"--nested-hw-virt\", \"on\"",
        "v.vmx[\"vhv.enable\"] = \"TRUE\"",
        "v.nested = true",
        "if %i[qemu utm].include?(machine.provider_name)",
    ] {
        assert!(v.contains(switch), "{switch}");
    }
    assert!(file(&nested, Target::Proxmox, "proxmox/main.tf")?.contains("type  = \"host\""));
    let module = |cloud: &str| cloud_module(&nested, cloud);
    let aws = module("aws")?.unwrap();
    assert!(aws.contains("\"m8i.large\"") && aws.contains("nested_virtualization = \"enabled\""), "{aws}");
    assert!(module("gcp")?.unwrap().contains("enable_nested_virtualization = true"));
    assert!(module("oci")?.unwrap().contains("are_virtual_instructions_enabled = true"));
    assert!(module("azure")?.is_none() && module("linode")?.is_none());
    assert_eq!(
        refusal(&spec("nested_virtualization: true")?, Target::Docker)?,
        "machines.a.hardware.nested_virtualization: only a VM gives it: a container has no kernel or firmware of its own"
    );
    Ok(())
}

#[test]
fn vms_get_gpus_on_proxmox_and_the_clouds() -> Outcome {
    let pinned = vm("gpus: { ids: ['0000:01:00.0'] }")?;
    let tf = file(&pinned, Target::Proxmox, "proxmox/main.tf")?;
    assert!(tf.contains("machine   = \"q35\"") || tf.contains("machine = \"q35\""), "{tf}");
    assert!(tf.contains("id     = \"0000:01:00.0\""), "{tf}");
    assert!(refusal(&pinned, Target::Vagrant)?.contains("Vagrant passes no GPU"));
    assert!(refusal(&pinned, Target::CloudVm)?.contains("give `gpus` as a count"));

    let counted = vm("gpus: 4")?;
    assert!(refusal(&counted, Target::Proxmox)?.contains("list them in `gpus.ids`"));
    let module = |cloud: &str| cloud_module(&counted, cloud);
    assert!(module("aws")?.unwrap().contains("\"g4dn.12xlarge\""));
    let gcp = module("gcp")?.unwrap();
    assert!(
        gcp.contains("\"g2-standard-48\"") && gcp.contains("on_host_maintenance = \"TERMINATE\""),
        "{gcp}"
    );
    assert!(module("oci")?.is_none(), "Oracle Cloud refuses GPUs");
    assert!(refusal(&vm("gpus: 9")?, Target::CloudVm)?.contains("at most 8 (g4dn.metal, NVIDIA T4)"));
    Ok(())
}
