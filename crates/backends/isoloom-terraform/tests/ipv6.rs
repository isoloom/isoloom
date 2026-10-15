//! IPv6 networks on the Terraform targets: Azure and Oracle Cloud write the spec's block, the
//! other clouds refuse with their own reason, and Proxmox's router carries the IPv6 side.

#![allow(clippy::unwrap_used)]

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};

const SPEC: &str = "version: 1
name: v6
networks:
  front: { cidr: 10.67.10.0/24, ipv6: { cidr: \"fd00:67:10::/64\" } }
  back: { cidr: 10.67.20.0/24, ipv6: { cidr: \"fd00:67:20::/64\" } }
reach: [{ from: front, to: back, ports: [80] }]
machines:
  web: { networks: { back: 10 }, ports: [{ port: 80 }], vm: { os: debian@12 } }
  user: { networks: { front: 10 }, vm: { os: debian@12 } }
";

fn env(target: Target) -> Environment {
    Environment::build(&isoloom_spec::parse(SPEC).unwrap(), target, None)
}

#[test]
fn azure_writes_the_block_the_interfaces_and_the_rules() {
    let azure = cloud("azure");
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
        let cloud = cloud(name);
        let refusal = cloud.refusal(&e).unwrap();
        assert!(refusal.starts_with("network `front` is IPv6: ") && refusal.contains(why), "{name}: {refusal}");
    }
}

#[test]
fn the_proxmox_router_takes_its_ipv6_address_and_forwards() {
    let files = isoloom_terraform::ProxmoxBackend.render(&env(Target::Proxmox)).unwrap();
    let tf = &files.iter().find(|f| f.path.ends_with("proxmox/main.tf")).unwrap().contents;
    assert!(tf.contains("Address=fd00:67:20::254/64"), "{tf}");
    assert!(tf.contains("net.ipv6.conf.all.forwarding=1"), "{tf}");
    assert!(tf.contains("ip6 saddr fd00:67:10::/64 ip6 daddr fd00:67:20::/64"), "{tf}");
}

fn cloud(name: &str) -> CloudVmModule {
    Cloud::ALL.into_iter().find(|c| c.id() == name).map(CloudVmModule::from).unwrap()
}

fn env_of(spec: &str) -> Environment {
    Environment::build(&isoloom_spec::parse(spec).unwrap(), Target::CloudVm, None)
}

#[test]
fn oracle_cloud_writes_the_block_the_vnics_and_the_rules() {
    let e = env(Target::CloudVm);
    let oci = cloud("oci");
    assert_eq!(oci.refusal(&e), None);
    let tf = &oci.render(&e).unwrap()[0].contents;
    // The spec's blocks alone: the VCN's private prefixes, no Oracle-allocated public one.
    assert!(tf.contains("is_ipv6enabled                   = true"), "{tf}");
    assert!(tf.contains("is_oracle_gua_allocation_enabled = false"), "{tf}");
    assert!(
        tf.contains("ipv6private_cidr_blocks          = [\"fd00:67:10::/64\", \"fd00:67:20::/64\"]"),
        "{tf}"
    );
    assert!(tf.contains("ipv6cidr_block    = \"fd00:67:20::/64\""), "{tf}");
    // The VNIC takes the derived address, which the IPv6 step puts on the interface.
    assert!(tf.contains("assign_ipv6ip          = true"), "{tf}");
    assert!(
        tf.contains("ipv6subnet_cidr = \"fd00:67:20::/64\"\n      ipv6address     = \"fd00:67:20::10\""),
        "{tf}"
    );
    assert!(tf.contains("ip -6 addr replace fd00:67:20::10/64"), "{tf}");
    // The rules mirror the IPv4 ones: the reach rule on its port, ICMPv6's "packet too big".
    assert!(
        tf.contains("description = \"reach from front\"\n    source      = \"fd00:67:10::/64\"\n    protocol    = \"6\""),
        "{tf}"
    );
    assert!(tf.contains("source      = \"::/0\"\n    protocol    = \"58\""), "{tf}");
    assert!(!tf.contains("ip -6 route"), "{tf}");
}

#[test]
fn oracle_cloud_reads_back_the_addresses_it_picks() {
    let spec = SPEC
        .replace("ipv6: { cidr: \"fd00:67:20::/64\" }", "ipv6: { cidr: \"fd00:67:20::/64\", addresses: auto }")
        .replace("user: { networks: { front: 10 }", "user: { networks: { front: 10, back: 20 }");
    let e = env_of(&spec);
    let oci = cloud("oci");
    assert_eq!(oci.refusal(&e), None);
    let tf = &oci.render(&e).unwrap()[0].contents;
    // No address named on `back`: Oracle Cloud picks it, and the set-up reads it back.
    assert!(!tf.contains("fd00:67:20::10"), "{tf}");
    assert!(
        tf.contains("oci_core_instance.web.create_vnic_details[0].ipv6address_ipv6subnet_cidr_pair_details[0].ipv6address"),
        "{tf}"
    );
    assert!(tf.contains("data.oci_core_vnic.user_back.ipv6addresses[0]"), "{tf}");
    assert!(tf.contains("inet 10.67.20.20/"), "{tf}");
    // Azure gives each interface its derived address, so it refuses.
    let azure = cloud("azure").refusal(&e).unwrap();
    assert!(azure.contains("`ipv6.addresses: auto`"), "{azure}");
}

#[test]
fn oracle_cloud_refuses_what_its_prefixes_cannot_hold() {
    let reserved = SPEC.replace("user: { networks: { front: 10 }", "user: { networks: { front: 3 }");
    let why = cloud("oci").refusal(&env_of(&reserved)).unwrap();
    assert!(why.contains("first four IPv6 addresses") && why.contains("fd00:67:10::3"), "{why}");
    let only = SPEC.replace("ipv6: { cidr: \"fd00:67:10::/64\" }", "ipv6: { cidr: \"fd00:67:10::/64\", only: true }");
    let why = cloud("oci").refusal(&env_of(&only)).unwrap();
    assert!(why.contains("IPv6-only"), "{why}");
}
