//! The clouds' networks from the plan: a network routed through its `gateway:` machine (AWS,
//! Azure, Google Cloud, Oracle Cloud), a network's MTU (Google Cloud sets it, the others refuse
//! it).

use crate::testing::{Outcome, plan};
use isoloom_plan::Environment;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};
use isoloom_test_support::{assert_every, hcl_attribute, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};

/// The plan of `yaml`, a valid spec, on `cloud-vm`.
fn env(yaml: &str) -> Outcome<Environment> {
    plan(yaml, Target::CloudVm)
}

/// `cloud`'s module for `e`.
fn module(cloud: Cloud, e: &Environment) -> Outcome<String> {
    Ok(CloudVmModule::new(cloud).render(e)?.contents)
}

/// Why `cloud` refuses `e`, if it does.
fn refusal(cloud: Cloud, e: &Environment) -> Option<String> {
    CloudVmModule::new(cloud).refusal(e)
}

const EDGE: &str = "
version: 1
name: edge
networks:
  out: { cidr: 10.70.0.0/24 }
  dmz: { cidr: 10.70.10.0/24, gateway: fw }
machines:
  fw:
    networks: { out: 5, dmz: 1 }
    os: debian@12
    app: { steps: [{ run: fw.sh }] }
  web:
    networks: { dmz: 10 }
    os: debian@12
    app: { steps: [{ run: web.sh }] }
";

/// `cloud`'s module for `e`, parsed.
fn parsed(cloud: Cloud, e: &Environment) -> Outcome<hcl::Body> {
    Ok(hcl_body(&module(cloud, e)?))
}

/// The one `resource "<kind>" "<name>"` of `tf`.
fn resource<'b>(tf: &'b hcl::Body, kind: &str, name: &str) -> &'b hcl::Body {
    hcl_one(tf, kind, name)
}

/// Whether one of the strings `body` holds anywhere (its user data, its scripts) holds `what`.
fn carries(body: &hcl::Body, what: &str) -> bool {
    hcl_literals(body).iter().any(|s| s.contains(what))
}

/// The routes of AWS route table `net`: each `route` block's destination and target interface.
fn aws_routes(tf: &hcl::Body, net: &str) -> Vec<(String, String)> {
    hcl_nested(resource(tf, "aws_route_table", net), "route")
        .iter()
        .filter(|r| hcl_attribute(r.body(), "network_interface_id").is_some())
        .map(|r| (hcl_text(r.body(), "cidr_block"), hcl_text(r.body(), "network_interface_id")))
        .collect()
}

#[test]
fn aws_routes_a_network_through_its_gateway_both_ways() -> Outcome {
    let tf = parsed(Cloud::Aws, &env(EDGE)?)?;
    // One VPC block holds both networks, so routes to each can be more specific than it.
    assert_eq!(hcl_text(resource(&tf, "aws_vpc", "env"), "cidr_block"), "10.70.0.0/20");
    assert_eq!(hcl_names(&tf, "resource", "aws_vpc_ipv4_cidr_block_association"), Vec::<String>::new());
    // dmz -> out through fw's dmz interface, out -> dmz through its out interface.
    assert_eq!(
        aws_routes(&tf, "dmz"),
        [("10.70.0.0/24".to_owned(), "aws_network_interface.fw_dmz.id".to_owned())]
    );
    assert_eq!(
        aws_routes(&tf, "out"),
        [("10.70.10.0/24".to_owned(), "aws_network_interface.fw_out.id".to_owned())]
    );
    // fw takes the router's address on dmz (AWS keeps .1), and the hosts files follow.
    assert_eq!(hcl_values(resource(&tf, "aws_network_interface", "fw_dmz"), "private_ips"), ["10.70.10.254"]);
    let fw = resource(&tf, "aws_instance", "fw");
    assert!(carries(fw, "10.70.10.254/24") && carries(fw, "net.ipv4.ip_forward=1"));
    assert!(carries(resource(&tf, "aws_instance", "web"), "10.70.10.254 fw"));
    Ok(())
}

#[test]
fn oracle_cloud_routes_a_network_through_its_gateway() -> Outcome {
    let tf = parsed(Cloud::Oci, &env(EDGE)?)?;
    assert_eq!(hcl_names(&tf, "data", "oci_core_private_ips"), ["fw_out", "fw_dmz"]);
    assert_eq!(hcl_names(&tf, "resource", "oci_core_route_table_attachment"), ["out", "dmz"]);
    let rules = hcl_nested(resource(&tf, "oci_core_route_table", "dmz"), "route_rules");
    let via: Vec<(String, String)> = rules
        .iter()
        .map(|r| (hcl_text(r.body(), "destination"), hcl_text(r.body(), "network_entity_id")))
        .collect();
    assert!(
        via.contains(&("10.70.0.0/24".to_owned(), "data.oci_core_private_ips.fw_dmz.private_ips[0].id".to_owned())),
        "{via:?}"
    );
    Ok(())
}

#[test]
fn azure_routes_a_network_through_its_gateway_both_ways() -> Outcome {
    let tf = parsed(Cloud::Azure, &env(EDGE)?)?;
    // fw takes the router's address on dmz (Azure keeps .1 to .3), forwarding on its interfaces.
    let nic = resource(&tf, "azurerm_network_interface", "fw_dmz");
    assert_eq!(hcl_text(nic, "ip_forwarding_enabled"), "true");
    assert_eq!(hcl_values(nic, "private_ip_address"), ["10.70.10.254"]);
    // dmz -> out through fw's dmz address, out -> dmz through its out address.
    let route = |id: &str| resource(&tf, "azurerm_route", id);
    assert_eq!(hcl_text(route("dmz_out"), "next_hop_in_ip_address"), "10.70.10.254");
    assert_eq!(hcl_text(route("out_dmz"), "next_hop_in_ip_address"), "10.70.0.5");
    assert_eq!(hcl_text(route("out_dmz"), "next_hop_type"), "VirtualAppliance");
    assert!(hcl_names(&tf, "resource", "azurerm_subnet_route_table_association").contains(&"dmz".to_owned()));
    assert!(carries(resource(&tf, "azurerm_linux_virtual_machine", "web"), "10.70.10.254 fw"));
    assert!(carries(resource(&tf, "azurerm_linux_virtual_machine", "fw"), "net.ipv4.ip_forward=1"));
    Ok(())
}

#[test]
fn google_cloud_routes_a_network_through_its_gateway_in_a_vpc_of_its_own() -> Outcome {
    let tf = parsed(Cloud::Gcp, &env(EDGE)?)?;
    // Google keeps .1 and the router's .254: fw takes the highest free address on dmz.
    let fw = resource(&tf, "google_compute_instance", "fw");
    assert_eq!(hcl_text(fw, "can_ip_forward"), "true");
    let addresses: Vec<String> = hcl_nested(fw, "network_interface").iter().map(|n| hcl_text(n.body(), "network_ip")).collect();
    assert_eq!(addresses, ["10.70.0.5", "10.70.10.252"]);
    assert!(carries(resource(&tf, "google_compute_instance", "web"), "10.70.10.252 fw"));
    // Each VPC sends the other network to fw's address in it; dmz is peered with none.
    let route = |id: &str| resource(&tf, "google_compute_route", id);
    assert_eq!(hcl_text(route("dmz_out"), "next_hop_ip"), "10.70.10.252");
    assert_eq!(hcl_text(route("dmz_out"), "network"), "google_compute_network.dmz.id");
    assert_eq!(hcl_text(route("out_dmz"), "next_hop_ip"), "10.70.0.5");
    assert_eq!(hcl_names(&tf, "resource", "google_compute_network_peering"), Vec::<String>::new());
    // Its second interface comes up by the MAC Google derives from its address.
    assert!(carries(fw, "42:01:0a:46:0a:fc"));
    Ok(())
}

#[test]
fn google_cloud_peers_the_networks_and_routes_a_routed_one_through_a_peer() -> Outcome {
    // `far` has no interface of fw's: it reaches dmz through out's route, over their peering.
    let spec = format!("{EDGE}  x:\n    networks: {{ far: 10 }}\n    os: debian@12\n    app: {{ steps: [{{ run: x.sh }}] }}\n").replace(
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }",
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }",
    );
    let tf = parsed(Cloud::Gcp, &env(&spec)?)?;
    let peering = resource(&tf, "google_compute_network_peering", "out_far");
    assert_eq!(hcl_text(peering, "export_custom_routes"), "true");
    assert_eq!(hcl_text(peering, "import_custom_routes"), "true");
    assert_eq!(hcl_names(&tf, "resource", "google_compute_network_peering"), ["out_far", "far_out"]);
    let routes = hcl_names(&tf, "resource", "google_compute_route");
    assert!(!routes.contains(&"far_dmz".to_owned()), "{routes:?}");
    assert_eq!(hcl_text(resource(&tf, "google_compute_route", "dmz_far"), "next_hop_ip"), "10.70.10.252");
    Ok(())
}

#[test]
fn google_cloud_refuses_more_interfaces_than_a_vm_takes() -> Outcome {
    let nets = (0..9).map(|i| format!("  n{i}: {{ cidr: 10.9{i}.0.0/24 }}\n")).collect::<Vec<_>>().concat();
    let on: Vec<String> = (0..9).map(|i| format!("n{i}: 10")).collect();
    let spec = format!(
        "version: 1\nname: wide\nnetworks:\n{nets}machines:\n  x:\n    networks: {{ {} }}\n    os: debian@12\n    app: {{ steps: [{{ run: x.sh }}] }}\n",
        on.join(", ")
    );
    let why = refusal(Cloud::Gcp, &env(&spec)?).ok_or("Google Cloud takes nine interfaces")?;
    assert!(why.contains("takes 8 network interfaces"), "{why}");
    // Three networks: a machine type with four vCPUs.
    let three = env(&EDGE.replace("dmz: 1 }", "dmz: 1, far: 10 }").replace(
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }",
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }",
    ))?;
    let tf = parsed(Cloud::Gcp, &three)?;
    assert_eq!(hcl_text(resource(&tf, "google_compute_instance", "fw"), "machine_type"), "e2-standard-4");
    Ok(())
}

#[test]
fn linode_refuses_a_gateway_with_its_reason() -> Outcome {
    // A gateway on two networks: refused for its second interface first.
    assert_eq!(
        refusal(Cloud::Linode, &env(EDGE)?).as_deref(),
        Some("machine `fw` is on several networks: a Linode instance has one VPC interface")
    );
    // On one network, the gateway itself is what Linode can't do.
    let one = env(&EDGE.replace("networks: { out: 5, dmz: 1 }", "networks: { dmz: 1 }"))?;
    assert_eq!(
        refusal(Cloud::Linode, &one).as_deref(),
        Some("a Linode VPC has no route tables, so no traffic can be sent through a machine")
    );
    Ok(())
}

const MTU: &str = "
version: 1
name: mtu
networks:
  a: { cidr: 10.80.0.0/24, mtu: 1400 }
  b: { cidr: 10.81.0.0/24 }
machines:
  x:
    networks: { a: 10 }
    os: debian@12
    app: { steps: [{ run: x.sh }] }
";

#[test]
fn google_cloud_sets_the_mtu_and_the_others_refuse_it() -> Outcome {
    let e = env(MTU)?;
    let mtus = |planned: &Environment| -> Outcome<Vec<String>> {
        let tf = parsed(Cloud::Gcp, planned)?;
        Ok(hcl_names(&tf, "resource", "google_compute_network")
            .iter()
            .map(|n| hcl_text(resource(&tf, "google_compute_network", n), "mtu"))
            .collect())
    };
    assert_eq!(mtus(&e)?, ["1400", "1500"]);
    for cloud in [Cloud::Aws, Cloud::Azure, Cloud::Oci] {
        assert_eq!(
            refusal(cloud, &e),
            Some(format!("network `a` sets its `mtu`: {cloud} has no setting for a network's MTU")),
            "{cloud}"
        );
    }
    // A VPC network each: each network its own MTU, in Google's range.
    let two = env(&MTU.replace("10.81.0.0/24 }", "10.81.0.0/24, mtu: 8896 }"))?;
    assert_eq!(mtus(&two)?, ["1400", "8896"]);
    let big = env(&MTU.replace("10.81.0.0/24 }", "10.81.0.0/24, mtu: 9000 }"))?;
    assert!(refusal(Cloud::Gcp, &big).ok_or("Google Cloud takes 9000")?.contains("1300 to 8896"));
    // Without one, Ethernet's 1500, as on every other target.
    let none = env(&MTU.replace(", mtu: 1400", ""))?;
    assert_eq!(mtus(&none)?, ["1500", "1500"]);
    Ok(())
}

/// A machine's Azure security group numbers its allow rules from 100 in steps of 10 below the
/// deny rule at 4000: a machine needing more than fit is refused, never given rules at or past
/// the deny (which never apply) or past 4096 (which Azure refuses).
#[test]
fn azure_refuses_a_machine_with_more_allow_rules_than_fit_below_its_deny_rule() -> Outcome {
    let spec = |ports: u16| {
        let list: Vec<String> = (1..=ports).map(|p| (2000 + p).to_string()).collect();
        format!(
            "version: 1\nname: many\nnetworks:\n  a: {{ cidr: 10.70.0.0/24 }}\n  b: {{ cidr: 10.70.1.0/24 }}\nreach: [{{ from: a, to: b, ports: [{}] }}]\nmachines:\n  x: {{ networks: {{ a: 10 }}, os: debian@12, app: {{ steps: [{{ run: x.sh }}] }} }}\n  y: {{ networks: {{ b: 10 }}, os: debian@12, app: {{ steps: [{{ run: y.sh }}] }} }}\n",
            list.join(", ")
        )
    };
    // y: its network, SSH, then each port in TCP and UDP: 2 + 2 * 190 = 382 rules fit (390).
    assert_eq!(refusal(Cloud::Azure, &env(&spec(190))?), None);
    let tf = parsed(Cloud::Azure, &env(&spec(190))?)?;
    let priorities: Vec<u64> = hcl_nested(resource(&tf, "azurerm_network_security_group", "y"), "security_rule")
        .iter()
        .filter(|r| hcl_text(r.body(), "access") == "Allow" && hcl_text(r.body(), "direction") == "Inbound")
        .map(|r| hcl_text(r.body(), "priority").parse().unwrap_or(u64::MAX))
        .collect();
    assert_eq!(priorities.len(), 382);
    assert_every!(priorities, |p: &u64| *p < 4000);
    // 2 + 2 * 200 = 402: past the deny rule.
    let why = refusal(Cloud::Azure, &env(&spec(200))?).ok_or("Azure takes 402 rules")?;
    assert!(
        why.starts_with("machine `y` needs 402 inbound rules on Azure") && why.contains("holds 390"),
        "{why}"
    );
    Ok(())
}
