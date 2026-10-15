//! The clouds' networks from the plan: a network routed through its `gateway:` machine (AWS,
//! Azure, Google Cloud, Oracle Cloud), a network's MTU (Google Cloud sets it, the others refuse
//! it).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::AwsBackend;
use isoloom_terraform::cloud_vm_others::azure::AzureBackend;
use isoloom_terraform::cloud_vm_others::gcp::GcpBackend;
use isoloom_terraform::cloud_vm_others::linode::LinodeBackend;
use isoloom_terraform::cloud_vm_others::oci::OciBackend;

fn env(spec: &str) -> Environment {
    let spec = isoloom_spec::parse(spec).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::CloudVm, None)
}

fn module(backend: &dyn Backend, e: &Environment) -> String {
    backend.render(e).unwrap().remove(0).contents
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
    vm: { os: debian@12, provision: [fw.sh] }
  web:
    networks: { dmz: 10 }
    vm: { os: debian@12, provision: [web.sh] }
";

#[test]
fn aws_routes_a_network_through_its_gateway_both_ways() {
    let tf = module(&AwsBackend, &env(EDGE));
    // One VPC block holds both networks, so routes to each can be more specific than it.
    assert!(tf.contains("cidr_block = \"10.70.0.0/20\""), "{tf}");
    assert!(!tf.contains("aws_vpc_ipv4_cidr_block_association"));
    // dmz -> out through fw's dmz interface, out -> dmz through its out interface.
    let table = |net: &str| {
        tf.split(&format!("resource \"aws_route_table\" \"{net}\""))
            .nth(1)
            .unwrap()
            .split("\n}\n")
            .next()
            .unwrap()
            .to_string()
    };
    assert!(table("dmz").contains("network_interface_id = aws_network_interface.fw_dmz.id"));
    assert!(table("out").contains("network_interface_id = aws_network_interface.fw_out.id"));
    // fw takes the router's address on dmz (AWS keeps .1), and the hosts files follow.
    assert!(tf.contains("\"10.70.10.254\""));
    assert!(tf.contains("10.70.10.254 fw"));
    assert!(tf.contains("net.ipv4.ip_forward=1"));
}

#[test]
fn oracle_cloud_routes_a_network_through_its_gateway() {
    let tf = module(&OciBackend, &env(EDGE));
    assert!(tf.contains("data \"oci_core_private_ips\" \"fw_dmz\""), "{tf}");
    assert!(tf.contains("resource \"oci_core_route_table_attachment\" \"dmz\""));
    assert!(tf.contains("network_entity_id = data.oci_core_private_ips.fw_dmz.private_ips[0].id"));
}

/// The block of `kind` named `name` in a module.
fn block(tf: &str, kind: &str, name: &str) -> String {
    let start = format!("resource \"{kind}\" \"{name}\"");
    tf.split(&start)
        .nth(1)
        .unwrap_or_else(|| panic!("no {start}:\n{tf}"))
        .split("\n}\n")
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn azure_routes_a_network_through_its_gateway_both_ways() {
    let tf = module(&AzureBackend, &env(EDGE));
    // fw takes the router's address on dmz (Azure keeps .1 to .3), forwarding on its interfaces.
    assert!(tf.contains("private_ip_address            = \"10.70.10.254\""), "{tf}");
    assert!(block(&tf, "azurerm_network_interface", "fw_dmz").contains("ip_forwarding_enabled = true"));
    // dmz -> out through fw's dmz address, out -> dmz through its out address.
    let route = |id: &str| block(&tf, "azurerm_route", id);
    assert!(route("dmz_out").contains("next_hop_in_ip_address = \"10.70.10.254\""));
    assert!(route("out_dmz").contains("next_hop_in_ip_address = \"10.70.0.5\""));
    assert!(route("out_dmz").contains("next_hop_type          = \"VirtualAppliance\""));
    assert!(tf.contains("resource \"azurerm_subnet_route_table_association\" \"dmz\""));
    assert!(tf.contains("10.70.10.254 fw"));
    assert!(tf.contains("net.ipv4.ip_forward=1"));
}

#[test]
fn google_cloud_routes_a_network_through_its_gateway_in_a_vpc_of_its_own() {
    let tf = module(&GcpBackend, &env(EDGE));
    // Google keeps .1 and the router's .254: fw takes the highest free address on dmz.
    assert!(tf.contains("network_ip = \"10.70.10.252\""), "{tf}");
    assert!(tf.contains("10.70.10.252 fw"));
    let fw = block(&tf, "google_compute_instance", "fw");
    assert!(fw.contains("can_ip_forward = true"));
    assert_eq!(fw.matches("network_interface {").count(), 2);
    // Each VPC sends the other network to fw's address in it; dmz is peered with none.
    let route = |id: &str| block(&tf, "google_compute_route", id);
    assert!(route("dmz_out").contains("next_hop_ip = \"10.70.10.252\""));
    assert!(route("dmz_out").contains("network     = google_compute_network.dmz.id"));
    assert!(route("out_dmz").contains("next_hop_ip = \"10.70.0.5\""));
    assert!(!tf.contains("google_compute_network_peering"));
    // Its second interface comes up by the MAC Google derives from its address.
    assert!(tf.contains("42:01:0a:46:0a:fc"));
}

#[test]
fn google_cloud_peers_the_networks_and_routes_a_routed_one_through_a_peer() {
    // `far` has no interface of fw's: it reaches dmz through out's route, over their peering.
    let spec = format!("{EDGE}  x:\n    networks: {{ far: 10 }}\n    vm: {{ os: debian@12, provision: [x.sh] }}\n").replace(
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }",
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }",
    );
    let tf = module(&GcpBackend, &env(&spec));
    let peering = block(&tf, "google_compute_network_peering", "out_far");
    assert!(
        peering.contains("export_custom_routes = true") && peering.contains("import_custom_routes = true"),
        "{peering}"
    );
    assert!(tf.contains("resource \"google_compute_network_peering\" \"far_out\""));
    assert!(!tf.contains("google_compute_network_peering\" \"dmz_"));
    assert!(!tf.contains("google_compute_route\" \"far_dmz\""));
    assert!(route_to(&tf, "dmz_far").contains("next_hop_ip = \"10.70.10.252\""));
}

fn route_to(tf: &str, id: &str) -> String {
    block(tf, "google_compute_route", id)
}

#[test]
fn google_cloud_refuses_more_interfaces_than_a_vm_takes() {
    let nets = (0..9).map(|i| format!("  n{i}: {{ cidr: 10.9{i}.0.0/24 }}\n")).collect::<Vec<_>>().concat();
    let on: Vec<String> = (0..9).map(|i| format!("n{i}: 10")).collect();
    let spec = format!(
        "version: 1\nname: wide\nnetworks:\n{nets}machines:\n  x:\n    networks: {{ {} }}\n    vm: {{ os: debian@12, provision: [x.sh] }}\n",
        on.join(", ")
    );
    let why = GcpBackend.refusal(&env(&spec)).unwrap();
    assert!(why.contains("8 interfaces at most"), "{why}");
    // Three networks: a machine type with four vCPUs.
    let three = env(&EDGE.replace("dmz: 1 }", "dmz: 1, far: 10 }").replace(
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }",
        "  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }",
    ));
    assert!(block(&module(&GcpBackend, &three), "google_compute_instance", "fw").contains("e2-standard-4"));
}

#[test]
fn linode_refuses_a_gateway_with_its_reason() {
    let why = LinodeBackend.refusal(&env(EDGE)).unwrap();
    assert!(why.contains("one VPC interface") || why.contains("route tables"), "{why}");
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
    vm: { os: debian@12, provision: [x.sh] }
";

#[test]
fn google_cloud_sets_the_mtu_and_the_others_refuse_it() {
    let e = env(MTU);
    assert!(module(&GcpBackend, &e).contains("mtu                     = 1400"));
    assert!(AwsBackend.refusal(&e).unwrap().contains("no setting for a network's MTU"));
    // A VPC network each: each network its own MTU, in Google's range.
    let two = env(&MTU.replace("10.81.0.0/24 }", "10.81.0.0/24, mtu: 8896 }"));
    let tf = module(&GcpBackend, &two);
    assert!(tf.contains("= 1400") && tf.contains("= 8896"), "{tf}");
    let big = env(&MTU.replace("10.81.0.0/24 }", "10.81.0.0/24, mtu: 9000 }"));
    assert!(GcpBackend.refusal(&big).unwrap().contains("1300 to 8896"));
    // Without one, Ethernet's 1500, as on every other target.
    let none = env(&MTU.replace(", mtu: 1400", ""));
    assert!(module(&GcpBackend, &none).contains("= 1500"));
}
