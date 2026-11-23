//! GPUs, host devices and nested virtualization on every target: rendered where the target can
//! give them, refused with the reason elsewhere.

use isoloom_spec::Target;
use isoloom_targets::{Error, GenerateError, Lab, generate};

use isoloom_test_support::{
    hcl_blocks, hcl_body, hcl_literals, hcl_nested, hcl_one, hcl_text, hcl_values, manifest, yaml, yaml_documents, yaml_str, yaml_strs,
};

use crate::testing::{Outcome, allowed, define, generated as file, lab, vagrantfile};

/// One machine with `hardware` set, as a container (an image) and as a VM (its OS and a step).
fn spec(hardware: &str) -> Outcome<Lab> {
    Ok(allowed(lab(&format!(
        "version: 1\nname: hw\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ os: debian@12, networks: {{ lab: 10 }}, app: {{ image: nginx, steps: [{{ run: a.sh }}] }}, hardware: {{ {hardware} }} }}\n"
    ))?))
}

/// One machine with `hardware` set, its OS and a step (a VM on the VM targets).
fn vm(hardware: &str) -> Outcome<Lab> {
    Ok(allowed(lab(&format!(
        "version: 1\nname: hw\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ os: debian@12, networks: {{ lab: 10 }}, app: {{ steps: [{{ run: a.sh }}] }}, hardware: {{ {hardware} }} }}\n"
    ))?))
}

/// Why `target` refuses the environment.
fn refusal(spec: &Lab, target: Target) -> Outcome<String> {
    match generate(spec, target) {
        Err(Error::Generate(GenerateError::Unsupported { what, .. })) => Ok(what),
        other => Err(format!("{target:?} doesn't refuse it: {:?}", other.map(|f| f.len())).into()),
    }
}

#[test]
fn a_container_gets_the_hosts_gpus_and_devices_on_docker_only() -> Outcome {
    let s = spec("gpus: { count: 2, capabilities: [compute] }, devices: ['/dev/fuse:/dev/fuse:rw']")?;
    let compose = yaml(&file(&s, Target::Docker, "docker/compose.yml")?);
    let device = |key: &str| yaml_str(&compose, &format!("services.a.devices.0.{key}"));
    assert_eq!([device("source"), device("target"), device("permissions")], ["/dev/fuse", "/dev/fuse", "rw"]);
    let gpus = "services.a.deploy.resources.reservations.devices.0";
    assert_eq!(yaml_strs(&compose, &format!("{gpus}.capabilities")), ["gpu", "compute"]);
    assert_eq!(
        [yaml_str(&compose, &format!("{gpus}.driver")), yaml_str(&compose, &format!("{gpus}.count"))],
        ["nvidia", "2"]
    );
    assert!(refusal(&s, Target::CloudDocker)?.contains("asks for GPUs (`gpus`): its container runs on a VM Isoloom creates"));
    let devices_only = spec("devices: [/dev/fuse]")?;
    assert!(refusal(&devices_only, Target::Kubernetes)?.contains("is given host devices (`devices`)"));
    Ok(())
}

#[test]
fn kubernetes_asks_the_device_plugin_for_a_count() -> Outcome {
    let s = spec("gpus: { count: 1, capabilities: [compute, utility] }")?;
    let manifests = file(&s, Target::Kubernetes, "kubernetes/environment.yaml")?;
    let docs = yaml_documents(&manifests);
    let container = &manifest(&docs, "Deployment", "a")["spec"]["template"]["spec"]["containers"][0];
    assert_eq!(container["resources"]["limits"]["nvidia.com/gpu"].as_str(), Some("1"), "{manifests}");
    let env: Vec<&str> = container["env"]
        .as_sequence()
        .into_iter()
        .flatten()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert!(env.contains(&"NVIDIA_DRIVER_CAPABILITIES"), "{env:?}");
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
    let vf = vagrantfile(&nested)?;
    let v = define(&vf, "a").every_statement();
    for switch in [
        "\"--nested-hw-virt\", \"on\"",
        "v.vmx[\"vhv.enable\"] = \"TRUE\"",
        "v.nested = true",
        "if %i[qemu utm].include?(machine.provider_name)",
    ] {
        assert!(v.iter().any(|s| s.contains(switch)), "{switch}: {v:?}");
    }
    let values = |tf: &str, key: &str| hcl_values(&hcl_body(tf), key);
    let proxmox = hcl_body(&file(&nested, Target::Proxmox, "proxmox/main.tf")?);
    let cpu = hcl_nested(hcl_one(&proxmox, "proxmox_virtual_environment_vm", "a"), "cpu");
    assert_eq!(
        cpu.first().map(|c| hcl_values(c.body(), "type")),
        Some(vec!["host".to_owned()]),
        "the VM's CPU is the host's"
    );
    let module = |cloud: &str| cloud_module(&nested, cloud);
    let aws = module("aws")?.ok_or("AWS nests")?;
    assert!(hcl_literals(&hcl_body(&aws)).contains(&"m8i.large".to_owned()), "{aws}");
    assert_eq!(values(&aws, "nested_virtualization"), ["enabled"]);
    assert_eq!(values(&module("gcp")?.ok_or("GCP nests")?, "enable_nested_virtualization"), ["true"]);
    assert_eq!(values(&module("oci")?.ok_or("OCI nests")?, "are_virtual_instructions_enabled"), ["true"]);
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
    let body = hcl_body(&tf);
    assert_eq!(hcl_values(&body, "machine"), ["q35"], "{tf}");
    let devices: Vec<String> = hcl_blocks(&body, "resource", &["proxmox_virtual_environment_vm"])
        .iter()
        .flat_map(|vm| hcl_nested(vm.body(), "hostpci"))
        .flat_map(|h| hcl_values(h.body(), "id"))
        .collect();
    assert_eq!(devices, ["0000:01:00.0"], "{tf}");
    assert!(refusal(&pinned, Target::Vagrant)?.contains("which this target doesn't pass to a VM"));
    assert!(refusal(&pinned, Target::CloudVm)?.contains("give `gpus` as a count"));

    let counted = vm("gpus: 4")?;
    assert!(refusal(&counted, Target::Proxmox)?.contains("list them in `gpus.ids`"));
    let module = |cloud: &str| cloud_module(&counted, cloud);
    let aws = hcl_body(&module("aws")?.ok_or("AWS gives GPUs")?);
    assert_eq!(hcl_text(hcl_one(&aws, "aws_instance", "a"), "instance_type"), "g4dn.12xlarge");
    let gcp = hcl_body(&module("gcp")?.ok_or("GCP gives GPUs")?);
    let instance = hcl_one(&gcp, "google_compute_instance", "a");
    assert_eq!(hcl_text(instance, "machine_type"), "g2-standard-48");
    let scheduling = hcl_nested(instance, "scheduling");
    assert_eq!(
        scheduling.first().map(|s| hcl_text(s.body(), "on_host_maintenance")).as_deref(),
        Some("TERMINATE")
    );
    assert!(module("oci")?.is_none(), "Oracle Cloud refuses GPUs");
    assert!(refusal(&vm("gpus: 9")?, Target::CloudVm)?.contains("at most 8 (g4dn.metal, NVIDIA T4)"));
    Ok(())
}
