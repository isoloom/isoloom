//! The Oracle Cloud (OCI) driver of the cloud-vm target: a single-network, multi-VM Linux lab
//! is accepted and keeps each machine's fixed private IP; multi-network labs are refused.

use isoloom_spec::Target;

use crate::testing::{Outcome, generated, lab, web_and_db};

const MULTI_NET: &str = r"version: 1
name: oci-multi
networks:
  front: { cidr: 10.61.10.0/24 }
  back:  { cidr: 10.61.20.0/24 }
machines:
  web:
    os: ubuntu@24.04
    networks: { front: 10, back: 10 }
    ports: { web: 80 }
    app: { steps: [{ run: p.sh }] }
";

/// The OCI module of `yaml`.
fn oci_tf(yaml: &str) -> Outcome<String> {
    generated(&lab(yaml)?, Target::CloudVm, "cloud-vm/oci/main.tf")
}

#[test]
fn single_network_linux_lab_builds_oci_instances_at_fixed_addresses() -> Outcome {
    let tf = oci_tf(&web_and_db("oci-test", "ubuntu@24.04"))?;
    // One OCI compute instance per machine.
    assert!(tf.contains("resource \"oci_core_instance\" \"web\""));
    assert!(tf.contains("resource \"oci_core_instance\" \"db\""));
    // Each keeps its spec address, pinned on the VNIC.
    assert!(tf.contains("= \"10.60.0.10\""));
    assert!(tf.contains("= \"10.60.0.11\""));
    // The network's security list, and the published port opened on its machine's own group.
    assert!(tf.contains("resource \"oci_core_security_list\" \"lab\""), "{tf}");
    assert!(tf.contains("min = 8080"));
    // The module outputs, the same shape the AWS driver gives.
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
    Ok(())
}

#[test]
fn a_machine_on_two_networks_gets_a_vnic_on_each() -> Outcome {
    let tf = oci_tf(MULTI_NET)?;
    // A subnet and a security list per network.
    for net in ["front", "back"] {
        assert!(tf.contains(&format!("resource \"oci_core_subnet\" \"{net}\"")), "{tf}");
        assert!(tf.contains(&format!("resource \"oci_core_security_list\" \"{net}\"")));
    }
    // The first network on the instance, the second on an attached VNIC found by MAC at boot.
    assert!(tf.contains("resource \"oci_core_vnic_attachment\" \"web_back\""));
    assert!(tf.contains("data.oci_core_vnic.web_back.mac_address"));
    assert!(tf.contains("skip_source_dest_check = true"));
    Ok(())
}
