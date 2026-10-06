//! The Oracle Cloud (OCI) driver of the cloud-vm target: a single-network, multi-VM Linux lab
//! is accepted and keeps each machine's fixed private IP; multi-network labs are refused.

use isoloom_core::{Target, generate, parse};

const LAB: &str = r#"version: 1
name: oci-test
networks:
  lab: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    services: [{ port: 80, http: true, publish: 8080 }]
    vm: { os: ubuntu-24.04, provision: [p.sh] }
  db:
    networks: { lab: 11 }
    services: [{ port: 5432, name: postgres }]
    vm: { os: ubuntu-24.04, provision: [p.sh] }
"#;

const MULTI_NET: &str = r#"version: 1
name: oci-multi
networks:
  front: { cidr: 10.61.10.0/24 }
  back:  { cidr: 10.61.20.0/24 }
machines:
  web:
    networks: { front: 10, back: 10 }
    services: [{ port: 80 }]
    vm: { os: ubuntu-24.04, provision: [p.sh] }
"#;

fn oci_tf(yaml: &str) -> String {
    let spec = parse(yaml).expect("parses");
    let files = generate(&spec, Target::CloudVm).expect("cloud-vm generates");
    files
        .into_iter()
        .find(|f| f.path.ends_with("cloud-vm/oci/main.tf"))
        .expect("an oci/main.tf for a single-network Linux lab")
        .contents
}

#[test]
fn single_network_linux_lab_builds_oci_instances_at_fixed_addresses() {
    let tf = oci_tf(LAB);
    // One OCI compute instance per machine.
    assert!(tf.contains("resource \"oci_core_instance\" \"web\""));
    assert!(tf.contains("resource \"oci_core_instance\" \"db\""));
    // Each keeps its spec address, pinned on the VNIC.
    assert!(tf.contains("private_ip       = \"10.60.0.10\""));
    assert!(tf.contains("private_ip       = \"10.60.0.11\""));
    // The VCN's security list, with the published port opened.
    assert!(tf.contains("resource \"oci_core_security_list\" \"env\""));
    assert!(tf.contains("min = 8080"));
    // The launcher outputs, the same shape the AWS driver gives.
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
}

#[test]
fn multi_network_lab_produces_no_oci_file() {
    // Multi-network is still valid cloud-vm (AWS handles it), but OCI refuses it: no oci file.
    let spec = parse(MULTI_NET).expect("parses");
    let files = generate(&spec, Target::CloudVm).expect("cloud-vm generates");
    assert!(
        files.iter().all(|f| !f.path.ends_with("cloud-vm/oci/main.tf")),
        "OCI should refuse a multi-network lab"
    );
}
