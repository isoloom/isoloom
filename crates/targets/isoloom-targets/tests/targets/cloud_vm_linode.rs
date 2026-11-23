//! The Linode driver of the `cloud-vm` target: a Linux environment gets a `linode/main.tf` with each
//! machine pinned to its spec address on its network's VPC subnet, behind a firewall of its own;
//! a machine on several networks (a Linode has one VPC interface) is refused, so no Linode file
//! is produced.

use isoloom_spec::{Target, parse};
use isoloom_targets::{Lab, generate};

use isoloom_test_support::{hcl, hcl_attribute, hcl_body, hcl_literals, hcl_names, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, web_and_db};

const LINODE: &str = ".isoloom/cloud-vm/linode/main.tf";

/// The Linode module of `spec`, when it is generated.
fn tf(spec: &isoloom_spec::Spec) -> Outcome<Option<String>> {
    let files = generate(&Lab::new(spec.clone()), Target::CloudVm)?;
    Ok(files.into_iter().find(|f| f.path == LINODE).map(|f| f.contents))
}

#[test]
fn single_network_linux_lab_pins_each_machine_on_a_vpc_subnet() -> Outcome {
    let spec = parse(&web_and_db("ln-lab", "debian@12"))?;

    let tf = tf(&spec)?.expect("Linode generates a file for a single-network Linux lab");
    let body = hcl_body(&tf);
    // One instance per machine.
    assert_eq!(hcl_names(&body, "resource", "linode_instance"), ["web", "db"]);
    // The VPC subnet holds the network's exact range.
    assert_eq!(hcl_text(hcl_one(&body, "linode_vpc_subnet", "lab"), "ipv4"), "10.60.0.0/24");
    // Each machine pinned to its spec address on the VPC interface.
    for (machine, address) in [("web", "10.60.0.10"), ("db", "10.60.0.11")] {
        let vm = hcl_one(&body, "linode_instance", machine);
        assert!(hcl_literals(vm).contains(&address.to_owned()), "{machine} at {address}: {tf}");
    }
    // Each instance behind its own firewall.
    for machine in ["web", "db"] {
        let firewall = hcl_one(&body, "linode_firewall", machine);
        let linodes: Vec<String> = match hcl_attribute(firewall, "linodes") {
            Some(hcl::Expression::Array(items)) => items.iter().map(ToString::to_string).collect(),
            _ => Vec::new(),
        };
        assert_eq!(linodes, [format!("linode_instance.{machine}.id")]);
    }
    // The expected outputs, shaped like the other clouds'.
    crate::testing::assert_cloud_outputs(&body);
    Ok(())
}

#[test]
fn networks_get_a_subnet_each_and_reach_both_ways() -> Outcome {
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
    os: debian@12
    networks: { a: 10 }
    ports: { web: 80 }
    app: { steps: [{ run: p.sh }] }
  db:
    os: debian@12
    networks: { b: 10 }
    ports: { db: 5432 }
    app: { steps: [{ run: p.sh }] }
",
    )
    .expect("spec parses");
    let tf = tf(&spec)?.expect("Linode takes machines on different networks");
    let body = hcl_body(&tf);
    assert_eq!(hcl_names(&body, "resource", "linode_vpc_subnet"), ["a", "b"]);
    // web may go to b on 5432 only; db may not start anything toward a.
    let labels = hcl_values(&body, "label");
    assert!(
        labels.contains(&"reach-b-tcp".to_owned()) && labels.contains(&"no-a-tcp".to_owned()),
        "{labels:?}"
    );
    // Routes to the other subnet through the VPC, at every boot.
    let routes: Vec<String> = hcl_literals(&body)
        .into_iter()
        .filter(|l| l.contains("ip route replace 10.61.0.0/24"))
        .collect();
    assert!(
        routes
            .iter()
            .any(|r| r.lines().any(|l| l.trim().starts_with("sudo ip route replace 10.61.0.0/24 via 10.60.0.1"))),
        "{routes:?}"
    );
    Ok(())
}

#[test]
fn a_machine_on_two_networks_is_refused_so_no_linode_file() -> Outcome {
    let spec = parse(
        r"
version: 1
name: ln-two
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { a: 10, b: 10 }
    ports: { web: 80 }
    app: { steps: [{ run: p.sh }] }
",
    )
    .expect("spec parses");

    assert!(tf(&spec)?.is_none(), "a Linode has one VPC interface, so no file is produced");
    Ok(())
}

#[test]
fn a_plan_fits_the_disk_too_and_a_disk_beyond_the_largest_is_refused() -> Outcome {
    let with = |resources: &str| {
        let mut spec = parse(&web_and_db("ln-lab", "debian@12"))?;
        spec.machines["web"].resources = serde_yaml_ng::from_str(resources)?;
        Ok::<_, Box<dyn std::error::Error>>(spec)
    };
    // 1 GB of memory fits the nanode; a 100 GB disk takes the 4 GB plan (80 GB is too small).
    let module = tf(&with("{ memory_mb: 1024, disk_gb: 100 }")?)?.ok_or("refused")?;
    let body = hcl_body(&module);
    assert_eq!(hcl_text(hcl_one(&body, "linode_instance", "web"), "type"), "g6-standard-4");
    let big = with("{ memory_mb: 1024, disk_gb: 400 }")?;
    assert!(tf(&big)?.is_none(), "a 400 GB disk is refused");
    let snapshot = isoloom_targets::resolved::resolve(&Lab::new(big));
    assert_eq!(
        snapshot
            .pointer("/targets/clouds/cloud-vm/refused/linode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default(),
        "machine `web` asks for a 400 GB disk: the largest Linode size here, `g6-standard-6`, has 320 GB"
    );
    Ok(())
}
