//! A data disk that is a resource of its own carries the environment's tags (labels on Google
//! Cloud), as its instance does: a reaper finds it by them.

pub mod testing;

use isoloom_terraform::Cloud;
use testing::{Outcome, cloud_vm, resource_body};

const SPEC: &str = "version: 1\nname: vols\nnetworks: { lab: { cidr: 10.60.0.0/24 } }\nmachines:\n  db:\n    os: debian@12\n    networks: { lab: 10 }\n    disks: { data: { size_gb: 20, mount: /data } }\n";

#[test]
fn every_clouds_volume_carries_the_environments_tags() -> Outcome {
    let gcp = cloud_vm(Cloud::Gcp, SPEC)?;
    assert!(
        resource_body(&gcp, "resource \"google_compute_disk\" \"db_data\"").contains("labels = {"),
        "{gcp}"
    );
    let digitalocean = cloud_vm(Cloud::DigitalOcean, SPEC)?;
    let volume = resource_body(&digitalocean, "resource \"digitalocean_volume\" \"db_data\"");
    assert!(
        volume.contains("tags") && volume.contains("concat([\"isoloom\", \"vols\", local.name]"),
        "{volume}"
    );
    let linode = cloud_vm(Cloud::Linode, SPEC)?;
    let linode_volume = resource_body(&linode, "resource \"linode_volume\" \"db_data\"");
    assert!(
        linode_volume.contains("tags") && linode_volume.contains("concat([\"isoloom\", \"vols\", local.name]"),
        "{linode_volume}"
    );
    Ok(())
}
