//! The controller's disk: the spec's `controller.resources.disk_gb` on every target that makes
//! one, else one default (an image's own floor above it, Azure's 30 GB, still holds).

pub mod testing;

use isoloom_terraform::Cloud;
use testing::{Outcome, cloud_vm, proxmox};

/// Two machines and a playbook (the controller runs it), with `controller` in the spec.
fn lab(controller: &str) -> String {
    format!(
        "version: 1\nname: c\nnetworks: {{ lab: {{ cidr: 10.60.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }} }}\n  db: {{ os: debian@12, networks: {{ lab: 20 }} }}\n{controller}provision: [{{ ansible: site.yml }}]\n"
    )
}

/// The body of `resource` (its header line) in `tf`, up to the next resource; empty when
/// there is none.
fn block<'a>(tf: &'a str, resource: &str) -> &'a str {
    tf.split(resource).nth(1).and_then(|rest| rest.split("\nresource ").next()).unwrap_or_default()
}

#[test]
fn the_controller_has_the_disk_the_spec_gives_it() -> Outcome {
    let sized = lab("controller: { resources: { disk_gb: 40 } }\n");
    let aws = cloud_vm(Cloud::Aws, &sized)?;
    assert!(
        block(&aws, "resource \"aws_instance\" \"isoloom_controller\"").contains("volume_size = 40"),
        "{aws}"
    );
    let gcp = cloud_vm(Cloud::Gcp, &sized)?;
    assert!(
        block(&gcp, "resource \"google_compute_instance\" \"isoloom_controller\"").contains("size  = 40"),
        "{gcp}"
    );
    let azure = cloud_vm(Cloud::Azure, &sized)?;
    assert!(
        block(&azure, "resource \"azurerm_linux_virtual_machine\" \"isoloom_controller\"").contains("disk_size_gb         = 40"),
        "{azure}"
    );
    Ok(())
}

#[test]
fn without_one_every_target_gives_the_same_default() -> Outcome {
    let plain = lab("");
    let aws = cloud_vm(Cloud::Aws, &plain)?;
    assert!(
        block(&aws, "resource \"aws_instance\" \"isoloom_controller\"").contains("volume_size = 10"),
        "{aws}"
    );
    let gcp = cloud_vm(Cloud::Gcp, &plain)?;
    assert!(
        block(&gcp, "resource \"google_compute_instance\" \"isoloom_controller\"").contains("size  = 10"),
        "{gcp}"
    );
    let pve = proxmox(&plain)?;
    assert!(
        block(&pve, "resource \"proxmox_virtual_environment_vm\" \"isoloom_controller\"").contains("size         = 10"),
        "{pve}"
    );
    Ok(())
}
