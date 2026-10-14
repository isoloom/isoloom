//! IPv6 networks on the Terraform targets: Azure writes the spec's block, the other clouds refuse
//! with their own reason, and Proxmox's router carries the IPv6 side.

#![allow(clippy::unwrap_used)]

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::CLOUDS;

const SPEC: &str = "version: 1
name: v6
networks:
  front: { cidr: 10.67.10.0/24, ipv6: { cidr: \"fd00:67:10::/64\" } }
  back: { cidr: 10.67.20.0/24, ipv6: { cidr: \"fd00:67:20::/64\" } }
reach: [{ from: front, to: back, ports: [80] }]
machines:
  web: { networks: { back: 10 }, services: [{ port: 80 }], vm: { os: debian-12 } }
  user: { networks: { front: 10 }, vm: { os: debian-12 } }
";

fn env(target: Target) -> Environment {
    Environment::build(&isoloom_spec::parse(SPEC).unwrap(), target, None)
}

#[test]
fn azure_writes_the_block_the_interfaces_and_the_rules() {
    let (_, azure) = CLOUDS.iter().find(|(n, _)| *n == "azure").unwrap();
    let e = env(Target::CloudVm);
    assert_eq!(azure.refusal(&e), None);
    let tf = &azure.render(&e).unwrap()[0].contents;
    assert!(tf.contains("\"10.67.20.0/24\", \"fd00:67:20::/64\""), "{tf}");
    assert!(tf.contains("private_ip_address_version    = \"IPv6\""), "{tf}");
    assert!(tf.contains("private_ip_address            = \"fd00:67:20::10\""), "{tf}");
    assert!(tf.contains("name                       = \"reach-0-front-ipv6-80-tcp\""), "{tf}");
    // The virtual network routes between subnets: no route through a router that isn't there.
    assert!(!tf.contains("ip -6 route"), "{tf}");
    assert!(tf.contains("ip -6 addr replace fd00:67:20::10/64"), "{tf}");
}

#[test]
fn the_other_clouds_refuse_with_their_reason() {
    let e = env(Target::CloudVm);
    for (name, why) in [("aws", "Amazon-provided"), ("gcp", "fd20::/20")] {
        let (_, cloud) = CLOUDS.iter().find(|(n, _)| *n == name).unwrap();
        let refusal = cloud.refusal(&e).unwrap();
        assert!(refusal.starts_with("network `front` is IPv6: ") && refusal.contains(why), "{name}: {refusal}");
    }
}

#[test]
fn the_proxmox_router_takes_its_ipv6_address_and_forwards() {
    let files = isoloom_terraform::proxmox::ProxmoxBackend.render(&env(Target::Proxmox)).unwrap();
    let tf = &files.iter().find(|f| f.path.ends_with("proxmox/main.tf")).unwrap().contents;
    assert!(tf.contains("Address=fd00:67:20::254/64"), "{tf}");
    assert!(tf.contains("net.ipv6.conf.all.forwarding=1"), "{tf}");
    assert!(tf.contains("ip6 saddr fd00:67:10::/64 ip6 daddr fd00:67:20::/64"), "{tf}");
}
