//! The clouds' networks from the plan: a network routed through its `gateway:` machine (AWS,
//! Oracle Cloud), a network's MTU (Google Cloud sets it, the others refuse it).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::AwsBackend;
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
    vm: { os: debian-12, provision: [fw.sh] }
  web:
    networks: { dmz: 10 }
    vm: { os: debian-12, provision: [web.sh] }
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
    vm: { os: debian-12, provision: [x.sh] }
";

#[test]
fn google_cloud_sets_the_mtu_and_the_others_refuse_it() {
    let e = env(MTU);
    assert!(module(&GcpBackend, &e).contains("mtu                     = 1400"));
    assert!(AwsBackend.refusal(&e).unwrap().contains("no setting for a network's MTU"));
    let two = env(&MTU.replace("10.81.0.0/24 }", "10.81.0.0/24, mtu: 9000 }"));
    assert!(GcpBackend.refusal(&two).unwrap().contains("one MTU"));
    // Without one, Ethernet's 1500, as on every other target.
    let none = env(&MTU.replace(", mtu: 1400", ""));
    assert!(module(&GcpBackend, &none).contains("= 1500"));
}
