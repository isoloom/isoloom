//! The Linode driver of the `cloud-vm` target: a Linux lab gets a `linode/main.tf` with each
//! machine pinned to its spec address on its network's VPC subnet, behind a firewall of its own;
//! a machine on several networks (a Linode has one VPC interface) is refused, so no Linode file
//! is produced.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_spec::{Target, parse};
use isoloom_targets::generate;

const LINODE: &str = ".isoloom/cloud-vm/linode/main.tf";

fn tf(spec: &isoloom_spec::Spec) -> Option<String> {
    generate(spec, Target::CloudVm)
        .unwrap()
        .into_iter()
        .find(|f| f.path == LINODE)
        .map(|f| f.contents)
}

#[test]
fn single_network_linux_lab_pins_each_machine_on_a_vpc_subnet() {
    let spec = parse(
        r"
version: 1
name: ln-lab
networks:
  lab: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    ports: [{ port: 80, http: true, publish: 8080 }]
    vm: { os: debian@12, provision: [p.sh] }
  db:
    networks: { lab: 11 }
    ports: [{ port: 5432, name: postgres }]
    vm: { os: ubuntu@24.04, provision: [p.sh] }
",
    )
    .expect("spec parses");

    let tf = tf(&spec).expect("Linode generates a file for a single-network Linux lab");
    // One instance per machine.
    assert!(tf.contains("resource \"linode_instance\" \"web\""), "{tf}");
    assert!(tf.contains("resource \"linode_instance\" \"db\""));
    // The VPC subnet holds the network's exact range.
    assert!(tf.contains("resource \"linode_vpc_subnet\" \"lab\""));
    assert!(tf.contains("ipv4   = \"10.60.0.0/24\""));
    // Each machine pinned to its spec address on the VPC interface.
    assert!(tf.contains("vpc = \"10.60.0.10\""), "{tf}");
    assert!(tf.contains("vpc = \"10.60.0.11\""));
    // Each instance behind its own firewall.
    assert!(tf.contains("resource \"linode_firewall\" \"web\""));
    assert!(tf.contains("linodes = [linode_instance.web.id]"));
    assert!(tf.contains("linodes = [linode_instance.db.id]"));
    // The expected outputs, shaped like the other clouds'.
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
}

#[test]
fn networks_get_a_subnet_each_and_reach_both_ways() {
    let spec = parse(
        r"
version: 1
name: ln-two
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
reach:
  - { from: a, to: b, ports: [5432] }
machines:
  web:
    networks: { a: 10 }
    ports: [{ port: 80 }]
    vm: { os: debian@12, provision: [p.sh] }
  db:
    networks: { b: 10 }
    ports: [{ port: 5432 }]
    vm: { os: debian@12, provision: [p.sh] }
",
    )
    .expect("spec parses");
    let tf = tf(&spec).expect("Linode takes machines on different networks");
    assert!(tf.contains("resource \"linode_vpc_subnet\" \"a\"") && tf.contains("resource \"linode_vpc_subnet\" \"b\""));
    // web may go to b on 5432 only; db may not start anything toward a.
    assert!(tf.contains("label    = \"reach-b-tcp\""), "{tf}");
    assert!(tf.contains("label    = \"no-a-tcp\""));
    // Routes to the other subnet through the VPC, at every boot.
    assert!(tf.contains("sudo ip route replace 10.61.0.0/24 via 10.60.0.1"));
}

#[test]
fn a_machine_on_two_networks_is_refused_so_no_linode_file() {
    let spec = parse(
        r"
version: 1
name: ln-two
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
machines:
  web:
    networks: { a: 10, b: 10 }
    ports: [{ port: 80 }]
    vm: { os: debian@12, provision: [p.sh] }
",
    )
    .expect("spec parses");

    assert!(tf(&spec).is_none(), "a Linode has one VPC interface, so no file is produced");
}
