//! The ESXi module on small plans: a vSwitch and a port group per network, a VM per machine set
//! up through the router, and the refusals.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use super::EsxiBackend;

fn env(spec: &str) -> Environment {
    let spec = isoloom_spec::parse(spec).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::Esxi, None)
}

const PAIR: &str = "
version: 1
name: pair
networks:
  front: { cidr: 10.20.1.0/24 }
  back: { cidr: 10.20.2.0/24 }
machines:
  web:
    networks: { front: 10, back: 10 }
    services: [{ port: 80, publish: 8080 }]
    vm: { os: ubuntu-24.04, provision: [web.sh] }
  db:
    networks: { back: 20 }
    vm: { os: debian-12, provision: [db.sh] }
";

fn module(spec: &str) -> String {
    let files = EsxiBackend.render(&env(spec)).unwrap();
    files.into_iter().find(|f| f.path.ends_with("esxi/main.tf")).unwrap().contents
}

#[test]
fn a_vswitch_and_a_port_group_per_network() {
    let tf = module(PAIR);
    for net in ["front", "back"] {
        assert!(tf.contains(&format!("resource \"esxi_vswitch\" \"{net}\"")), "{net}");
        assert!(tf.contains(&format!("resource \"esxi_portgroup\" \"{net}\"")), "{net}");
    }
    assert!(tf.contains("vswitch = esxi_vswitch.back.name"));
}

#[test]
fn a_vm_per_machine_set_up_through_the_router() {
    let tf = module(PAIR);
    for vm in ["isoloom_router", "web", "db"] {
        assert!(tf.contains(&format!("resource \"esxi_guest\" \"{vm}\"")), "{vm}");
    }
    assert!(tf.contains("resource \"terraform_data\" \"web\""));
    assert!(tf.contains("bastion_host        = local.router_address"));
    assert!(tf.contains("router_address = esxi_guest.isoloom_router.ip_address"));
    // Debian has no default OVA: the variable says it must be given.
    assert!(tf.contains("images needs an OVA for each of: debian-12, ubuntu-24.04."));
    assert!(tf.contains("noble-server-cloudimg-amd64.ova"));
}

#[test]
fn windows_and_ipv6_are_refused() {
    let windows = PAIR.replace("os: debian-12", "os: windows-server-2022");
    let why = EsxiBackend.refusal(&env(&windows)).unwrap();
    assert!(why.contains("no ESXi image for `windows-server-2022`"), "{why}");
    let v6 = PAIR.replace("back: { cidr: 10.20.2.0/24 }", "back: { cidr: 10.20.2.0/24, ipv6: { cidr: \"fd00:20::/64\" } }");
    let why = EsxiBackend.refusal(&env(&v6)).unwrap();
    assert!(why.contains("IPv6"), "{why}");
    assert_eq!(EsxiBackend.refusal(&env(PAIR)), None);
}
