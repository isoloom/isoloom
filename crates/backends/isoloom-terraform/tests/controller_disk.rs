//! The controller's disk: the spec's `controller.resources.disk_gb` on every target that makes
//! one, else one default (an image's own floor above it, Azure's 30 GB, still holds).

pub mod testing;

use isoloom_terraform::Cloud;
use testing::{Outcome, cloud_vm, proxmox, resource_body};

/// Two machines and a playbook (the controller runs it), with `controller` in the spec.
fn lab(controller: &str) -> String {
    format!(
        "version: 1\nname: c\nnetworks: {{ lab: {{ cidr: 10.60.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }} }}\n  db: {{ os: debian@12, networks: {{ lab: 20 }} }}\n{controller}provision: [{{ ansible: site.yml }}]\n"
    )
}

/// `hcl` with each run of spaces one space: an attribute as written, whatever its block aligns.
fn aligned(hcl: &str) -> String {
    hcl.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

#[test]
fn the_controller_has_the_disk_the_spec_gives_it() -> Outcome {
    let sized = lab("controller: { resources: { disk_gb: 40 } }\n");
    let aws = cloud_vm(Cloud::Aws, &sized)?;
    assert!(
        resource_body(&aws, "resource \"aws_instance\" \"isoloom_controller\"").contains("volume_size = 40"),
        "{aws}"
    );
    let gcp = cloud_vm(Cloud::Gcp, &sized)?;
    assert!(
        aligned(resource_body(&gcp, "resource \"google_compute_instance\" \"isoloom_controller\"")).contains("size = 40"),
        "{gcp}"
    );
    let azure = cloud_vm(Cloud::Azure, &sized)?;
    assert!(
        resource_body(&azure, "resource \"azurerm_linux_virtual_machine\" \"isoloom_controller\"").contains("disk_size_gb         = 40"),
        "{azure}"
    );
    Ok(())
}

#[test]
fn without_one_every_target_gives_the_same_default() -> Outcome {
    let plain = lab("");
    let aws = cloud_vm(Cloud::Aws, &plain)?;
    assert!(
        resource_body(&aws, "resource \"aws_instance\" \"isoloom_controller\"").contains("volume_size = 10"),
        "{aws}"
    );
    let gcp = cloud_vm(Cloud::Gcp, &plain)?;
    assert!(
        aligned(resource_body(&gcp, "resource \"google_compute_instance\" \"isoloom_controller\"")).contains("size = 10"),
        "{gcp}"
    );
    let pve = proxmox(&plain)?;
    assert!(
        resource_body(&pve, "resource \"proxmox_virtual_environment_vm\" \"isoloom_controller\"").contains("size         = 10"),
        "{pve}"
    );
    Ok(())
}
