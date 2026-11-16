//! IPv6 networks on the Terraform targets: Azure and Oracle Cloud write the spec's block, the
//! other clouds refuse with their own reason, and Proxmox's router carries the IPv6 side.

use crate::testing::{Outcome, plan, proxmox, texts};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};
use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_nested, hcl_one, hcl_text, hcl_values};

/// The scripts (set-up, user data) of `kind` VM `name` in `tf`.
fn scripts(tf: &str, kind: &str, name: &str) -> Vec<String> {
    texts(tf, kind, name).into_iter().filter(|t| t.contains('\n')).collect()
}

/// Whether one of `scripts` has a line holding `what`.
fn has_line(scripts: &[String], what: &str) -> bool {
    scripts.iter().flat_map(|s| s.lines()).any(|l| l.contains(what))
}

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
    let body = hcl_body(tf);
    let back = hcl_one(&body, "azurerm_subnet", "back");
    assert_eq!(hcl_values(back, "address_prefixes"), ["10.67.20.0/24", "fd00:67:20::/64"]);
    // Each machine's interface has an IPv6 configuration at its address.
    let v6: Vec<(String, String)> = hcl_blocks(&body, "resource", &["azurerm_network_interface"])
        .iter()
        .flat_map(|nic| hcl_nested(nic.body(), "ip_configuration"))
        .filter(|c| hcl_values(c.body(), "private_ip_address_version") == ["IPv6"])
        .map(|c| (hcl_text(c.body(), "private_ip_address"), hcl_text(c.body(), "private_ip_address_allocation")))
        .collect();
    // web on back, user on front, each at its fixed address.
    assert_eq!(
        v6,
        [
            ("fd00:67:20::10".to_owned(), "Static".to_owned()),
            ("fd00:67:10::10".to_owned(), "Static".to_owned())
        ]
    );
    let rules = hcl_blocks(&body, "resource", &["azurerm_network_security_group"])
        .iter()
        .flat_map(|g| hcl_values(g.body(), "name"))
        .collect::<Vec<_>>();
    assert!(rules.iter().any(|r| r == "reach-0-front-ipv6-80-tcp"), "{rules:?}");
    // web's set-up puts its address on its interface; the virtual network routes between
    // subnets: no route through a router that isn't there.
    let web = scripts(tf, "azurerm_linux_virtual_machine", "web");
    assert!(has_line(&web, "ip -6 addr replace fd00:67:20::10/64"), "{web:?}");
    assert!(!has_line(&web, "ip -6 route"), "{web:?}");
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
    let router = texts(&proxmox(SPEC)?, "proxmox_virtual_environment_file", "isoloom_router");
    // Its interface on back (a networkd unit), forwarding, and the reach rule in nftables.
    for line in [
        "Address=fd00:67:20::254/64",
        "net.ipv6.conf.all.forwarding=1",
        "ip6 saddr fd00:67:10::/64 ip6 daddr fd00:67:20::/64",
    ] {
        assert!(has_line(&router, line), "{line}: {router:?}");
    }
    Ok(())
}

#[test]
fn oracle_cloud_writes_the_block_the_vnics_and_the_rules() -> Outcome {
    let e = plan(SPEC, Target::CloudVm)?;
    let oci = CloudVmModule::new(Cloud::Oci);
    assert_eq!(oci.refusal(&e), None);
    let tf = &oci.render(&e)?.contents;
    let body = hcl_body(tf);
    // The spec's blocks alone: the VCN's private prefixes, no Oracle-allocated public one.
    let vcn = hcl_blocks(&body, "resource", &["oci_core_vcn"]);
    let vcn = vcn.first().ok_or("no VCN")?.body();
    assert_eq!(hcl_text(vcn, "is_ipv6enabled"), "true");
    assert_eq!(hcl_text(vcn, "is_oracle_gua_allocation_enabled"), "false");
    assert_eq!(hcl_values(vcn, "ipv6private_cidr_blocks"), ["fd00:67:10::/64", "fd00:67:20::/64"]);
    assert_eq!(hcl_values(hcl_one(&body, "oci_core_subnet", "back"), "ipv6cidr_block"), ["fd00:67:20::/64"]);
    // The VNIC takes the derived address, which the IPv6 step puts on the interface.
    let details: Vec<(String, String)> = hcl_blocks(&body, "resource", &["oci_core_instance"])
        .iter()
        .flat_map(|i| hcl_nested(i.body(), "create_vnic_details"))
        .filter(|v| hcl_values(v.body(), "assign_ipv6ip") == ["true"])
        .flat_map(|v| hcl_nested(v.body(), "ipv6address_ipv6subnet_cidr_pair_details"))
        .map(|p| (hcl_text(p.body(), "ipv6subnet_cidr"), hcl_text(p.body(), "ipv6address")))
        .collect();
    assert_eq!(
        details,
        [
            ("fd00:67:20::/64".to_owned(), "fd00:67:20::10".to_owned()),
            ("fd00:67:10::/64".to_owned(), "fd00:67:10::10".to_owned())
        ]
    );
    let web = scripts(tf, "oci_core_instance", "web");
    assert!(has_line(&web, "ip -6 addr replace fd00:67:20::10/64"), "{web:?}");
    assert!(!has_line(&web, "ip -6 route"), "{web:?}");
    // The rules mirror the IPv4 ones: the reach rule on its port, ICMPv6's "packet too big".
    let ingress: Vec<(String, String)> = hcl_blocks(&body, "resource", &["oci_core_security_list"])
        .iter()
        .flat_map(|l| hcl_nested(l.body(), "ingress_security_rules"))
        .map(|r| (hcl_values(r.body(), "source").join(""), hcl_values(r.body(), "protocol").join("")))
        .collect();
    for wanted in [("fd00:67:10::/64", "6"), ("::/0", "58")] {
        assert!(ingress.iter().any(|(s, p)| (s.as_str(), p.as_str()) == wanted), "{wanted:?}: {ingress:?}");
    }
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
    // No address named on `back`: Oracle Cloud picks it, and each machine's own set-up reads it
    // from its metadata service, by its VNIC's IPv4 address there.
    let body = hcl_body(tf);
    assert!(!hcl_literals(&body).iter().any(|l| l.contains("fd00:67:20::10")), "an address on back");
    for (name, address) in [("web", "10.67.20.10"), ("user", "10.67.20.20")] {
        let own = scripts(tf, "oci_core_instance", name);
        assert!(has_line(&own, &format!("v.get(\"privateIp\") == \"{address}\"")), "{name}: {own:?}");
        assert!(has_line(&own, "ipv6Addresses"), "{name}: {own:?}");
    }
    // user's second interface, on back, comes up by its IPv4 address.
    assert!(has_line(&scripts(tf, "oci_core_instance", "user"), "inet 10.67.20.20/"));
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
    let unchecked = crate::testing::planned(&isoloom_plan::Lab::parse(&only).map_err(|p| format!("{p:?}"))?, Target::CloudVm)?;
    let ipv6_only = CloudVmModule::new(Cloud::Oci).refusal(&unchecked).unwrap();
    assert!(ipv6_only.contains("IPv6-only"), "{ipv6_only}");
    Ok(())
}
