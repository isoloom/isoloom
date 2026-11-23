//! A data disk that is a resource of its own carries the environment's tags (labels on Google
//! Cloud), as its instance does: a reaper finds it by them.

use crate::testing::{Outcome, cloud_vm};
use isoloom_terraform::Cloud;
use isoloom_test_support::{hcl_map, hcl_resource, hcl_values};

const SPEC: &str = "version: 1\nname: vols\nnetworks: { lab: { cidr: 10.60.0.0/24 } }\nmachines:\n  db:\n    os: debian@12\n    networks: { lab: 10 }\n    disks: { data: { size_gb: 20, mount: /data } }\n";

#[test]
fn a_google_cloud_volume_carries_the_environments_labels() -> Outcome {
    let disk = hcl_resource(&cloud_vm(Cloud::Gcp, SPEC)?, "google_compute_disk", "db_data");
    let labels = hcl_map(&disk, "labels");
    assert!(labels.contains(&("isoloom-environment".to_owned(), "vols".to_owned())), "{labels:?}");
    Ok(())
}

#[test]
fn a_digitalocean_and_a_linode_volume_carry_the_environments_tags() -> Outcome {
    for (cloud, kind) in [(Cloud::DigitalOcean, "digitalocean_volume"), (Cloud::Linode, "linode_volume")] {
        let volume = hcl_resource(&cloud_vm(cloud, SPEC)?, kind, "db_data");
        let tags = hcl_values(&volume, "tags");
        assert!(tags.starts_with(&["isoloom".to_owned(), "vols".to_owned()]), "{cloud}: {tags:?}");
    }
    Ok(())
}
