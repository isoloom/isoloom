//! The private DNS zone (`dns.private`) on the Terraform targets: each cloud with a DNS of its
//! own writes the zone, bound to the environment's network, and a record per name and alias;
//! the clouds with none and Proxmox carry the names in each VM's hosts file; with the zone off,
//! nothing of it is written.

#![allow(clippy::unwrap_used)]

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::CLOUDS;

const SPEC: &str = "version: 1
name: names
dns: { private: true, domain: corp.internal }
networks:
  office: { cidr: 10.84.0.0/24 }
machines:
  web: { networks: { office: 10 }, aliases: [www], services: [{ port: 80 }], vm: { os: debian-12, provision: [web.sh] } }
";

fn env(spec: &str, target: Target) -> Environment {
    Environment::build(&isoloom_spec::parse(spec).unwrap(), target, None)
}

fn module(cloud: &str, spec: &str) -> String {
    let (_, backend) = CLOUDS.iter().find(|(n, _)| *n == cloud).unwrap();
    backend.render(&env(spec, Target::CloudVm)).unwrap()[0].contents.clone()
}

#[test]
fn each_cloud_with_a_dns_writes_the_zone_and_a_record_per_name() {
    for (cloud, zone, record) in [
        ("aws", "resource \"aws_route53_zone\" \"private\"", "name    = \"www.corp.internal\""),
        (
            "azure",
            "resource \"azurerm_private_dns_zone_virtual_network_link\" \"private\"",
            "name                = \"www\"",
        ),
        ("gcp", "visibility = \"private\"", "name         = \"www.corp.internal.\""),
        ("oci", "resource \"oci_dns_resolver\" \"env\"", "domain          = \"www.corp.internal\""),
    ] {
        let tf = module(cloud, SPEC);
        assert!(tf.contains(zone), "{cloud}: {tf}");
        assert!(tf.contains(record), "{cloud}: {tf}");
        assert!(tf.contains("\"10.84.0.10\""), "{cloud}: {tf}");
    }
    assert!(module("aws", SPEC).contains("enable_dns_hostnames = true"));
}

#[test]
fn off_no_zone_is_written() {
    let off = SPEC.replace("dns: { private: true, domain: corp.internal }\n", "");
    for cloud in ["aws", "azure", "gcp", "oci"] {
        let tf = module(cloud, &off);
        assert!(
            !tf.contains("corp.internal") && !tf.contains("dns_zone") && !tf.contains("route53"),
            "{cloud}: {tf}"
        );
    }
}

#[test]
fn proxmox_takes_the_domain_as_its_search_domain() {
    let files = isoloom_terraform::proxmox::ProxmoxBackend.render(&env(SPEC, Target::Proxmox)).unwrap();
    let tf = &files.iter().find(|f| f.path.ends_with("proxmox/main.tf")).unwrap().contents;
    assert!(tf.contains("domain  = \"corp.internal\""), "{tf}");
}
