//! The Linode driver of the `cloud-vm` target: a Linux environment gets a `linode/main.tf` with each
//! machine pinned to its spec address on its network's VPC subnet, behind a firewall of its own;
//! a machine on several networks (a Linode has one VPC interface) is refused, so no Linode file
//! is produced.

use isoloom_spec::{Target, parse};
use isoloom_targets::{Lab, generate};

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
    assert!(tf.contains("resource \"linode_vpc_subnet\" \"a\"") && tf.contains("resource \"linode_vpc_subnet\" \"b\""));
    // web may go to b on 5432 only; db may not start anything toward a.
    assert!(tf.contains("label    = \"reach-b-tcp\""), "{tf}");
    assert!(tf.contains("label    = \"no-a-tcp\""));
    // Routes to the other subnet through the VPC, at every boot.
    assert!(tf.contains("sudo ip route replace 10.61.0.0/24 via 10.60.0.1"));
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
    let web = module.split("resource \"linode_instance\" \"web\"").nth(1).ok_or("no web instance")?;
    assert!(web.contains("type            = \"g6-standard-4\""), "{web}");
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
