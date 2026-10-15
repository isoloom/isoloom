//! IPv6 networks on the Terraform targets: Azure and Oracle Cloud write the spec's block, the
//! other clouds refuse with their own reason, and Proxmox's router carries the IPv6 side.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, ProxmoxBackend};
use testing::{Outcome, file, plan};

const SPEC: &str = "version: 1
name: v6
networks:
  front: { cidr: 10.67.10.0/24, ipv6: { cidr: \"fd00:67:10::/64\" } }
  back: { cidr: 10.67.20.0/24, ipv6: { cidr: \"fd00:67:20::/64\" } }
reach: [{ from: front, to: back, ports: [80] }]
machines:
  web: { networks: { back: 10 }, ports: { web: 80 }, os: debian@12 }
  user: { networks: { front: 10 }, os: debian@12 }
";

#[test]
fn azure_writes_the_block_the_interfaces_and_the_rules() -> Outcome {
    let azure = CloudVmModule::new(Cloud::Azure);
    let e = plan(SPEC, Target::CloudVm)?;
    assert_eq!(azure.refusal(&e), None);
    let tf = &azure.render(&e)?.contents;
    assert!(tf.contains("\"10.67.20.0/24\", \"fd00:67:20::/64\""), "{tf}");
    assert!(tf.contains("private_ip_address_version    = \"IPv6\""), "{tf}");
    assert!(tf.contains("private_ip_address            = \"fd00:67:20::10\""), "{tf}");
    assert!(tf.contains("name                       = \"reach-0-front-ipv6-80-tcp\""), "{tf}");
    // The virtual network routes between subnets: no route through a router that isn't there.
    assert!(!tf.contains("ip -6 route"), "{tf}");
    assert!(tf.contains("ip -6 addr replace fd00:67:20::10/64"), "{tf}");
    Ok(())
}

#[test]
fn the_other_clouds_refuse_with_their_reason() -> Outcome {
    let e = plan(SPEC, Target::CloudVm)?;
    for (name, why) in [(Cloud::Aws, "Amazon-provided"), (Cloud::Gcp, "fd20::/20")] {
        let cloud = CloudVmModule::new(name);
        let refusal = cloud.refusal(&e).unwrap();
        assert!(refusal.starts_with("network `front` is IPv6: ") && refusal.contains(why), "{name}: {refusal}");
    }
    Ok(())
}

#[test]
fn the_proxmox_router_takes_its_ipv6_address_and_forwards() -> Outcome {
    let files = ProxmoxBackend.render(&plan(SPEC, Target::Proxmox)?)?;
    let tf = file(&files, "proxmox/main.tf")?;
    assert!(tf.contains("Address=fd00:67:20::254/64"), "{tf}");
    assert!(tf.contains("net.ipv6.conf.all.forwarding=1"), "{tf}");
    assert!(tf.contains("ip6 saddr fd00:67:10::/64 ip6 daddr fd00:67:20::/64"), "{tf}");
    Ok(())
}

#[test]
fn oracle_cloud_writes_the_block_the_vnics_and_the_rules() -> Outcome {
    let e = plan(SPEC, Target::CloudVm)?;
    let oci = CloudVmModule::new(Cloud::Oci);
    assert_eq!(oci.refusal(&e), None);
    let tf = &oci.render(&e)?.contents;
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
    Ok(())
}

#[test]
fn oracle_cloud_reads_back_the_addresses_it_picks() -> Outcome {
    let spec = SPEC
        .replace("ipv6: { cidr: \"fd00:67:20::/64\" }", "ipv6: { cidr: \"fd00:67:20::/64\", addresses: auto }")
        .replace("user: { networks: { front: 10 }", "user: { networks: { front: 10, back: 20 }");
    let e = plan(&spec, Target::CloudVm)?;
    let oci = CloudVmModule::new(Cloud::Oci);
    assert_eq!(oci.refusal(&e), None);
    let tf = &oci.render(&e)?.contents;
    // No address named on `back`: Oracle Cloud picks it, and the set-up reads it back.
    assert!(!tf.contains("fd00:67:20::10"), "{tf}");
    assert!(
        tf.contains("oci_core_instance.web.create_vnic_details[0].ipv6address_ipv6subnet_cidr_pair_details[0].ipv6address"),
        "{tf}"
    );
    assert!(tf.contains("data.oci_core_vnic.user_back.ipv6addresses[0]"), "{tf}");
    assert!(tf.contains("inet 10.67.20.20/"), "{tf}");
    // Azure gives each interface its derived address, so it refuses.
    let azure = CloudVmModule::new(Cloud::Azure).refusal(&e).unwrap();
    assert!(azure.contains("`ipv6.addresses: auto`"), "{azure}");
    Ok(())
}

#[test]
fn oracle_cloud_refuses_what_its_prefixes_cannot_hold() -> Outcome {
    let reserved = SPEC.replace("user: { networks: { front: 10 }", "user: { networks: { front: 3 }");
    let why = CloudVmModule::new(Cloud::Oci).refusal(&plan(&reserved, Target::CloudVm)?).unwrap();
    assert!(why.contains("first four IPv6 addresses") && why.contains("fd00:67:10::3"), "{why}");
    let only = SPEC.replace("ipv6: { cidr: \"fd00:67:10::/64\" }", "ipv6: { cidr: \"fd00:67:10::/64\", only: true }");
    // Not a valid spec (an IPv6-only network has no way out): the cloud refuses it all the same.
    let unchecked = testing::planned(&isoloom_plan::Lab::parse(&only).map_err(|p| format!("{p:?}"))?, Target::CloudVm);
    let ipv6_only = CloudVmModule::new(Cloud::Oci).refusal(&unchecked).unwrap();
    assert!(ipv6_only.contains("IPv6-only"), "{ipv6_only}");
    Ok(())
}
