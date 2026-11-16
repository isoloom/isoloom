//! The private DNS zone (`dns.private`) on the Terraform targets: each cloud with a DNS of its
//! own writes the zone, bound to the environment's network, and a record per name and alias;
//! the clouds with none and Proxmox carry the names in each VM's hosts file; with the zone off,
//! nothing of it is written.

use crate::testing::{Outcome, cloud_vm, proxmox};
use isoloom_terraform::Cloud;
use isoloom_test_support::{assert_every, hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_one, hcl_resource, hcl_text, hcl_values};

/// The bodies of the resources of `body` whose attribute `key` (anywhere in them) is `value`.
fn with_value<'b>(body: &'b hcl::Body, key: &str, value: &str) -> Vec<&'b hcl::Body> {
    hcl_blocks(body, "resource", &[])
        .into_iter()
        .map(hcl::Block::body)
        .filter(|b| hcl_values(b, key).iter().any(|v| v == value))
        .collect()
}

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
    // Each cloud's zone (bound to the network), and the record of the alias at web's address.
    for (cloud, zone, key, alias) in [
        (Cloud::Aws, ("aws_route53_zone", "private"), "name", "www.corp.internal"),
        (Cloud::Azure, ("azurerm_private_dns_zone_virtual_network_link", "private"), "name", "www"),
        (Cloud::Gcp, ("google_dns_managed_zone", "private"), "name", "www.corp.internal."),
        (Cloud::Oci, ("oci_dns_resolver", "env"), "domain", "www.corp.internal"),
    ] {
        let body = hcl_body(&cloud_vm(cloud, SPEC)?);
        assert_eq!(hcl_blocks(&body, "resource", &[zone.0, zone.1]).len(), 1, "{cloud}: {zone:?}");
        let records = with_value(&body, key, alias);
        assert!(!records.is_empty(), "{cloud}: no record {key} = {alias}");
        assert_every!(records, |r: &&hcl::Body| hcl_literals(r).iter().any(|l| l == "10.84.0.10"));
    }
    let gcp = hcl_body(&cloud_vm(Cloud::Gcp, SPEC)?);
    assert_eq!(hcl_text(hcl_one(&gcp, "google_dns_managed_zone", "private"), "visibility"), "private");
    let vpc = hcl_resource(&cloud_vm(Cloud::Aws, SPEC)?, "aws_vpc", "env");
    assert_eq!(hcl_text(&vpc, "enable_dns_hostnames"), "true");
    Ok(())
}

#[test]
fn azure_writes_aaaa_records_on_ipv6_and_reverse_names() -> Outcome {
    let v6 = SPEC.replace("10.84.0.0/24 }", "10.84.0.0/24, ipv6: { cidr: \"fd00:84::/64\" } }");
    let body = hcl_body(&cloud_vm(Cloud::Azure, &v6)?);
    let aaaa = hcl_one(&body, "azurerm_private_dns_aaaa_record", "www_corp_internal");
    assert_eq!(hcl_values(aaaa, "records"), ["fd00:84::10"]);
    // The reverse zone of the network's /24, linked to the virtual network; one PTR per machine,
    // naming the machine (not its aliases).
    assert_eq!(with_value(&body, "name", "0.84.10.in-addr.arpa").len(), 1);
    assert!(hcl_names(&body, "resource", "azurerm_private_dns_zone_virtual_network_link").contains(&"reverse_0_84_10".to_owned()));
    assert_eq!(hcl_names(&body, "resource", "azurerm_private_dns_ptr_record"), ["web_corp_internal"]);
    assert_eq!(
        hcl_values(hcl_one(&body, "azurerm_private_dns_ptr_record", "web_corp_internal"), "records"),
        ["web.corp.internal"]
    );
    // IPv4 only: no AAAA record.
    assert_eq!(
        hcl_names(&hcl_body(&cloud_vm(Cloud::Azure, SPEC)?), "resource", "azurerm_private_dns_aaaa_record"),
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn off_no_zone_is_written() -> Outcome {
    let off = SPEC.replace("dns: { private: true, domain: corp.internal }\n", "");
    for cloud in [Cloud::Aws, Cloud::Azure, Cloud::Gcp, Cloud::Oci] {
        let tf = hcl_body(&cloud_vm(cloud, &off)?);
        let kinds: Vec<&str> = hcl_blocks(&tf, "resource", &[])
            .iter()
            .filter_map(|b| b.labels().first().map(hcl::BlockLabel::as_str))
            .collect();
        assert_every!(&kinds, |k: &&&str| !k.contains("dns") && !k.contains("route53"));
        assert!(!hcl_literals(&tf).iter().any(|l| l.contains("corp.internal")), "{cloud}: the domain is written");
    }
    Ok(())
}

#[test]
fn proxmox_takes_the_domain_as_its_search_domain() -> Outcome {
    let web = hcl_resource(&proxmox(SPEC)?, "proxmox_virtual_environment_vm", "web");
    assert_eq!(hcl_values(&web, "domain"), ["corp.internal"]);
    Ok(())
}
