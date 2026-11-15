//! The Oracle Cloud (OCI) driver of the cloud-vm target: a single-network, multi-VM Linux environment
//! is accepted and keeps each machine's fixed private IP; multi-network environments are refused.

use isoloom_spec::Target;

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};

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
    let body = hcl_body(&tf);
    // One OCI compute instance per machine.
    assert_eq!(hcl_names(&body, "resource", "oci_core_instance"), ["web", "db"]);
    // Each keeps its spec address, pinned on the VNIC.
    for (machine, address) in [("web", "10.60.0.10"), ("db", "10.60.0.11")] {
        let vnic = hcl_nested(hcl_one(&body, "oci_core_instance", machine), "create_vnic_details");
        assert_eq!(
            vnic.first().map(|v| hcl_values(v.body(), "private_ip")),
            Some(vec![address.to_owned()]),
            "{machine}"
        );
    }
    // The network's security list, and the published port opened on its machine's own group.
    assert_eq!(hcl_names(&body, "resource", "oci_core_security_list"), ["lab"]);
    assert!(hcl_values(&body, "min").contains(&"8080".to_owned()), "{tf}");
    // The module outputs, the same shape the AWS driver gives.
    assert_eq!(hcl_blocks(&body, "output", &["ip"]).len(), 1);
    let ready = hcl_blocks(&body, "output", &["ready_file"]);
    assert_eq!(ready.first().map(|o| hcl_text(o.body(), "value")).as_deref(), Some("/var/lib/isoloom/ready"));
    Ok(())
}

#[test]
fn a_machine_on_two_networks_gets_a_vnic_on_each() -> Outcome {
    let tf = oci_tf(MULTI_NET)?;
    let body = hcl_body(&tf);
    // A subnet and a security list per network.
    for kind in ["oci_core_subnet", "oci_core_security_list"] {
        let mut names = hcl_names(&body, "resource", kind);
        names.sort();
        assert_eq!(names, ["back", "front"], "{kind}");
    }
    // The first network on the instance, the second on an attached VNIC, found at boot by its
    // MAC, which the instance reads from its metadata service (the VNIC is attached once the
    // instance exists, so Terraform can't hand it over).
    assert_eq!(hcl_names(&body, "resource", "oci_core_vnic_attachment"), ["web_back"]);
    let scripts: Vec<String> = hcl_literals(&body).into_iter().filter(|l| l.contains('\n')).collect();
    assert!(scripts.iter().any(|s| s.contains("http://169.254.169.254/opc/v2/vnics/")), "{tf}");
    assert!(hcl_values(&body, "skip_source_dest_check").iter().all(|v| v == "true"), "{tf}");
    assert_ne!(hcl_values(&body, "skip_source_dest_check"), Vec::<String>::new());
    Ok(())
}
