//! The Linode driver of the `cloud-vm` target: a single-network Linux lab gets a `linode/main.tf`
//! with each machine pinned to its spec address on a VPC subnet; multi-network (and other
//! out-of-scope) specs are refused, so no Linode file is produced.

use isoloom_core::{Target, generate, parse};

const LINODE: &str = ".isoloom/cloud-vm/linode/main.tf";

fn tf(spec: &isoloom_core::Spec) -> Option<String> {
    generate(spec, Target::CloudVm)
        .unwrap()
        .into_iter()
        .find(|f| f.path == LINODE)
        .map(|f| f.contents)
}

#[test]
fn single_network_linux_lab_pins_each_machine_on_a_vpc_subnet() {
    let spec = parse(
        r#"
version: 1
name: ln-lab
networks:
  lab: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    services: [{ port: 80, http: true, publish: 8080 }]
    vm: { os: debian-12, provision: [p.sh] }
  db:
    networks: { lab: 11 }
    services: [{ port: 5432, name: postgres }]
    vm: { os: ubuntu-24.04, provision: [p.sh] }
"#,
    )
    .expect("spec parses");

    let tf = tf(&spec).expect("Linode generates a file for a single-network Linux lab");
    // One instance per machine.
    assert!(tf.contains("resource \"linode_instance\" \"web\""), "{tf}");
    assert!(tf.contains("resource \"linode_instance\" \"db\""));
    // The VPC subnet holds the network's exact range.
    assert!(tf.contains("resource \"linode_vpc_subnet\" \"env\""));
    assert!(tf.contains("ipv4   = \"10.60.0.0/24\""));
    // Each machine pinned to its spec address on the VPC interface.
    assert!(tf.contains("vpc = \"10.60.0.10\""), "{tf}");
    assert!(tf.contains("vpc = \"10.60.0.11\""));
    // The firewall gates the instances.
    assert!(tf.contains("resource \"linode_firewall\" \"env\""));
    assert!(tf.contains("linodes = [linode_instance.web.id, linode_instance.db.id]"));
    // The expected outputs, shaped like the other clouds'.
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
}

#[test]
fn multi_network_lab_is_refused_so_no_linode_file() {
    let spec = parse(
        r#"
version: 1
name: ln-two
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
machines:
  web:
    networks: { a: 10 }
    services: [{ port: 80 }]
    vm: { os: debian-12 }
  db:
    networks: { b: 10 }
    services: [{ port: 5432 }]
    vm: { os: debian-12 }
"#,
    )
    .expect("spec parses");

    assert!(tf(&spec).is_none(), "Linode refuses a multi-network lab, so no file is produced");
}
