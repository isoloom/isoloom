//! The private DNS zone (`dns.private`) on the Terraform targets: each cloud with a DNS of its
//! own writes the zone, bound to the environment's network, and a record per name and alias;
//! the clouds with none and Proxmox carry the names in each VM's hosts file; with the zone off,
//! nothing of it is written.

pub mod testing;

use isoloom_terraform::Cloud;
use testing::{Outcome, cloud_vm, proxmox};

const SPEC: &str = "version: 1
name: names
dns: { private: true, domain: corp.internal }
networks:
  office: { cidr: 10.84.0.0/24 }
machines:
  web: { networks: { office: 10 }, aliases: [www], ports: { web: 80 }, os: debian@12, app: { steps: [{ run: web.sh }] } }
";

#[test]
fn each_cloud_with_a_dns_writes_the_zone_and_a_record_per_name() -> Outcome {
    for (cloud, zone, record) in [
        (Cloud::Aws, "resource \"aws_route53_zone\" \"private\"", "name    = \"www.corp.internal\""),
        (
            Cloud::Azure,
            "resource \"azurerm_private_dns_zone_virtual_network_link\" \"private\"",
            "name                = \"www\"",
        ),
        (Cloud::Gcp, "visibility = \"private\"", "name         = \"www.corp.internal.\""),
        (Cloud::Oci, "resource \"oci_dns_resolver\" \"env\"", "domain          = \"www.corp.internal\""),
    ] {
        let tf = cloud_vm(cloud, SPEC)?;
        assert!(tf.contains(zone), "{cloud}: {tf}");
        assert!(tf.contains(record), "{cloud}: {tf}");
        assert!(tf.contains("\"10.84.0.10\""), "{cloud}: {tf}");
    }
    assert!(cloud_vm(Cloud::Aws, SPEC)?.contains("enable_dns_hostnames = true"));
    Ok(())
}

#[test]
fn azure_writes_aaaa_records_on_ipv6_and_reverse_names() -> Outcome {
    let v6 = SPEC.replace("10.84.0.0/24 }", "10.84.0.0/24, ipv6: { cidr: \"fd00:84::/64\" } }");
    let tf = cloud_vm(Cloud::Azure, &v6)?;
    assert!(tf.contains("resource \"azurerm_private_dns_aaaa_record\" \"www_corp_internal\""), "{tf}");
    assert!(tf.contains("records             = [\"fd00:84::10\"]"), "{tf}");
    // The reverse zone of the network's /24, linked to the virtual network; one PTR per machine,
    // naming the machine (not its aliases).
    assert!(tf.contains("name                = \"0.84.10.in-addr.arpa\""), "{tf}");
    assert!(tf.contains("resource \"azurerm_private_dns_zone_virtual_network_link\" \"reverse_0_84_10\""));
    assert!(tf.contains("resource \"azurerm_private_dns_ptr_record\" \"web_corp_internal\""));
    assert!(tf.contains("records             = [\"web.corp.internal\"]"));
    assert!(!tf.contains("azurerm_private_dns_ptr_record\" \"www_corp_internal\""));
    // IPv4 only: no AAAA record.
    assert!(!cloud_vm(Cloud::Azure, SPEC)?.contains("azurerm_private_dns_aaaa_record"));
    Ok(())
}

#[test]
fn off_no_zone_is_written() -> Outcome {
    let off = SPEC.replace("dns: { private: true, domain: corp.internal }\n", "");
    for cloud in [Cloud::Aws, Cloud::Azure, Cloud::Gcp, Cloud::Oci] {
        let tf = cloud_vm(cloud, &off)?;
        assert!(
            !tf.contains("corp.internal") && !tf.contains("dns_zone") && !tf.contains("route53"),
            "{cloud}: {tf}"
        );
    }
    Ok(())
}

#[test]
fn proxmox_takes_the_domain_as_its_search_domain() -> Outcome {
    let tf = proxmox(SPEC)?;
    assert!(tf.contains("domain  = \"corp.internal\""), "{tf}");
    Ok(())
}
