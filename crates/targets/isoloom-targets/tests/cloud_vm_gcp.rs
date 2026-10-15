//! The GCP driver for the `cloud-vm` target: it produces a valid-shaped `google` module for the
//! specs it supports, a VPC network per network, and declines Windows (which comes later).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{Target, load};
use isoloom_targets::{Lab, generate};

fn gcp(name: &str) -> Option<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    let lab = Lab::new(load(&dir).expect("example parses"));
    let files = generate(&lab, Target::CloudVm).expect("cloud-vm generates");
    files.into_iter().find(|f| f.path.ends_with("cloud-vm/gcp/main.tf")).map(|f| f.contents)
}

#[test]
fn gcp_uses_the_google_provider_and_static_internal_ips() {
    let tf = gcp("hello-stack").expect("hello-stack has a gcp file");
    assert!(
        tf.contains("google = { source = \"hashicorp/google\", version = \"~> 8.5.0\" }"),
        "google provider"
    );
    assert!(tf.contains("resource \"google_compute_network\" \"app\""));
    assert!(tf.contains("resource \"google_compute_subnetwork\""));
    // Every address of the spec is kept: web sits at .10 on 10.60.0.0/24, pinned on its NIC.
    assert!(tf.contains("network_ip = \"10.60.0.10\""), "static internal address");
    assert!(tf.contains("network_ip = \"10.60.0.20\""));
    // The public address comes from the instance's access config.
    assert!(tf.contains("network_interface[0].access_config[0].nat_ip"));
    // The same outputs the AWS driver gives.
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("output \"machines\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
    // web publishes 8080: a firewall opening and a published output.
    assert!(tf.contains("resource \"google_compute_firewall\" \"web_published\""));
    assert!(tf.contains("output \"published\""));
}

#[test]
fn gcp_opens_reach_rules_across_networks() {
    // segmented has three networks and `reach` rules: a VPC network each, peered both ways.
    let tf = gcp("segmented").expect("segmented has a gcp file");
    // front may reach back on 6379 only: a firewall on the cache, from front's range.
    assert!(tf.contains("resource \"google_compute_firewall\" \"cache_reach_front\""));
    assert!(tf.contains("ports    = [\"6379\"]"));
    // The subnetworks carry each network's exact range.
    assert!(tf.contains("ip_cidr_range = \"10.61.20.0/24\""));
    assert!(tf.contains("resource \"google_compute_network_peering\" \"front_back\""));
    assert!(tf.contains("resource \"google_compute_network_peering\" \"back_front\""));
    // The rule opening the cache to front is in the cache's network's VPC.
    let rule = tf.split("resource \"google_compute_firewall\" \"cache_reach_front\"").nth(1).unwrap();
    assert!(
        rule.split("\n}\n").next().unwrap().contains("network       = google_compute_network.back.id"),
        "{rule}"
    );
}

#[test]
fn gcp_puts_a_machine_on_several_networks_with_an_interface_in_each_vpc() {
    // vlan-office's admin is on several networks: an interface in each VPC, forwarding on.
    let tf = gcp("vlan-office").expect("vlan-office has a gcp file");
    let admin = tf.split("resource \"google_compute_instance\" \"admin\"").nth(1).unwrap();
    let admin = admin.split("\n}\n").next().unwrap();
    assert!(admin.matches("network_interface {").count() > 1, "{admin}");
    assert!(admin.contains("can_ip_forward = true"));
    assert_eq!(admin.matches("access_config {").count(), 1, "the public address on the first only");
}

#[test]
fn gcp_declines_windows() {
    // windows-hello is Windows-only: AWS builds it, the GCP driver declines it for now.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/windows-hello");
    let lab = Lab::new(load(&dir).expect("parses"));
    let files = generate(&lab, Target::CloudVm).expect("cloud-vm generates");
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/aws/main.tf")));
    assert!(
        !files.iter().any(|f| f.path.ends_with("cloud-vm/gcp/main.tf")),
        "Windows on GCP comes later, so no gcp file"
    );
}
