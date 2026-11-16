//! The controller's disk: the spec's `controller.resources.disk_gb` on every target that makes
//! one, else one default (an image's own floor above it, Azure's 30 GB, still holds).

use crate::testing::{Outcome, cloud_vm, proxmox};
use isoloom_terraform::Cloud;
use isoloom_test_support::{hcl_resource, hcl_values};

/// Two machines and a playbook (the controller runs it), with `controller` in the spec.
fn lab(controller: &str) -> String {
    format!(
        "version: 1\nname: c\nnetworks: {{ lab: {{ cidr: 10.60.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }} }}\n  db: {{ os: debian@12, networks: {{ lab: 20 }} }}\n{controller}provision: [{{ ansible: site.yml }}]\n"
    )
}

/// The values of attribute `size` of the controller, a `kind` resource of `tf`, wherever it is set
/// (its root disk's block).
fn controller_disk(tf: &str, kind: &str, size: &str) -> Vec<String> {
    hcl_values(&hcl_resource(tf, kind, "isoloom_controller"), size)
}

#[test]
fn the_controller_has_the_disk_the_spec_gives_it() -> Outcome {
    let sized = lab("controller: { resources: { disk_gb: 40 } }\n");
    assert_eq!(controller_disk(&cloud_vm(Cloud::Aws, &sized)?, "aws_instance", "volume_size"), ["40"]);
    assert_eq!(controller_disk(&cloud_vm(Cloud::Gcp, &sized)?, "google_compute_instance", "size"), ["40"]);
    assert_eq!(
        controller_disk(&cloud_vm(Cloud::Azure, &sized)?, "azurerm_linux_virtual_machine", "disk_size_gb"),
        ["40"]
    );
    Ok(())
}

#[test]
fn without_one_every_target_gives_the_same_default() -> Outcome {
    let plain = lab("");
    assert_eq!(controller_disk(&cloud_vm(Cloud::Aws, &plain)?, "aws_instance", "volume_size"), ["10"]);
    assert_eq!(controller_disk(&cloud_vm(Cloud::Gcp, &plain)?, "google_compute_instance", "size"), ["10"]);
    assert_eq!(controller_disk(&proxmox(&plain)?, "proxmox_virtual_environment_vm", "size"), ["10"]);
    Ok(())
}
