//! A machine's size on the clouds whose sizes come from a table: the smallest with its CPUs,
//! memory, disk (where the size fixes it) and an interface per network; more than the largest
//! has is refused, naming the machine and that size, never silently given less. Oracle Cloud's
//! flexible shape is sized to the machine.

pub mod testing;

use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmBackend};
use testing::{Outcome, cloud_vm, plan};

/// One Linux machine with `memory` and `extra` in its spec.
fn lab(memory: &str, extra: &str) -> String {
    sized(&format!("memory_mb: {memory}"), extra)
}

/// One Linux machine with `resources` (`cpus: 4`) and `extra` in its spec.
fn sized(resources: &str, extra: &str) -> String {
    format!(
        "version: 1\nname: big\nnetworks: {{ lab: {{ cidr: 10.60.0.0/24 }} }}\nmachines:\n  db:\n    os: debian@12\n    networks: {{ lab: 10 }}\n    resources: {{ {resources} }}\n{extra}    app: {{ steps: [{{ run: p.sh }}] }}\n"
    )
}

/// The size attribute each cloud's module gives `db`.
fn size_of(cloud: Cloud, yaml: &str) -> Outcome<String> {
    size_in(cloud, &cloud_vm(cloud, yaml)?)
}

/// The first size attribute of `cloud` in `tf`.
fn size_in(cloud: Cloud, tf: &str) -> Outcome<String> {
    let key = match cloud {
        Cloud::Aws => "instance_type",
        Cloud::Gcp => "machine_type",
        Cloud::Linode => "type",
        _ => "size",
    };
    let line = tf
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with(&format!("{key} ")) && l.contains('"'))
        .ok_or_else(|| format!("{cloud:?}: no {key} in {tf}"))?;
    Ok(line.split('"').nth(1).unwrap_or_default().to_owned())
}

#[test]
fn a_machine_gets_a_size_with_as_many_vcpus_as_it_asks_for() -> Outcome {
    let four = sized("cpus: 4, memory_mb: 1024", "");
    for (cloud, size) in [
        (Cloud::Aws, "m7i-flex.xlarge"),
        (Cloud::Gcp, "e2-standard-4"),
        (Cloud::Azure, "Standard_B4s_v2"),
        (Cloud::DigitalOcean, "s-4vcpu-8gb"),
        (Cloud::Linode, "g6-standard-4"),
    ] {
        assert_eq!(size_of(cloud, &four)?, size, "{cloud:?}");
    }
    // An AMD OCPU is two vCPUs, and the shape takes 1 GB an OCPU at least.
    let oci = cloud_vm(Cloud::Oci, &sized("cpus: 8, memory_mb: 1024", ""))?;
    assert!(oci.contains("ocpus         = 4") && oci.contains("memory_in_gbs = 4"), "{oci}");
    let one = cloud_vm(Cloud::Oci, &sized("cpus: 1, memory_mb: 16384", ""))?;
    assert!(one.contains("ocpus         = 2") && one.contains("memory_in_gbs = 16"), "{one}");
    Ok(())
}

#[test]
fn more_cpus_than_a_clouds_largest_size_is_refused() -> Outcome {
    let got = refusals(&sized("cpus: 64", ""))?;
    let why = |cloud: Cloud| got.iter().find(|(c, _)| *c == cloud).and_then(|(_, w)| w.clone()).unwrap_or_default();
    for (cloud, size) in [
        (Cloud::Aws, "`m7i-flex.xlarge`, has 4 vCPUs"),
        (Cloud::Gcp, "`e2-standard-8`, has 8 vCPUs"),
        (Cloud::Azure, "`Standard_D32as_v5`, has 32 vCPUs"),
        (Cloud::DigitalOcean, "`s-8vcpu-16gb`, has 8 vCPUs"),
        (Cloud::Linode, "`g6-standard-6`, has 6 vCPUs"),
    ] {
        let why = why(cloud);
        assert!(why.starts_with("machine `db` asks for 64 CPUs") && why.ends_with(size), "{cloud:?}: {why}");
    }
    assert_eq!(why(Cloud::Oci), "", "64 CPUs are 32 OCPUs");
    let oci = refusals(&sized("cpus: 200", ""))?;
    let oci = oci.iter().find(|(c, _)| *c == Cloud::Oci).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert!(oci.contains("machine `db` asks for 100 OCPUs") && oci.contains("64 OCPUs (128 CPUs)"), "{oci}");
    Ok(())
}

#[test]
fn a_disk_picks_the_size_that_comes_with_as_much_or_is_refused() -> Outcome {
    let big = sized("disk_gb: 100", "");
    assert_eq!(size_of(Cloud::DigitalOcean, &big)?, "s-4vcpu-8gb");
    assert_eq!(size_of(Cloud::Linode, &big)?, "g6-standard-4");
    let got = refusals(&sized("disk_gb: 400", ""))?;
    let why = |cloud: Cloud| got.iter().find(|(c, _)| *c == cloud).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert_eq!(
        why(Cloud::DigitalOcean),
        "machine `db` asks for a 400 GB disk: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 320 GB"
    );
    assert!(why(Cloud::Linode).ends_with("`g6-standard-6`, has 320 GB"), "{}", why(Cloud::Linode));
    assert_eq!(why(Cloud::Aws), "", "an AWS disk is the machine's own");
    Ok(())
}

/// An environment of `networks` networks: `db` on every one, and a controller when `provision`.
fn wide(networks: usize, provision: bool) -> String {
    let nets: Vec<String> = (0..networks).map(|i| format!("  n{i}: {{ cidr: 10.6{i}.0.0/24 }}")).collect();
    let on: Vec<String> = (0..networks).map(|i| format!("n{i}: 10")).collect();
    let provision = if provision { "provision:\n  - ansible: provision/site.yml\n" } else { "" };
    format!(
        "version: 1\nname: wide\nnetworks:\n{}\nmachines:\n  db: {{ os: debian@12, networks: {{ {} }}, app: {{ steps: [{{ run: p.sh }}] }} }}\n{provision}",
        nets.join("\n"),
        on.join(", ")
    )
}

#[test]
fn an_aws_machine_gets_a_type_with_an_interface_per_network() -> Outcome {
    assert_eq!(size_of(Cloud::Aws, &wide(2, false))?, "t3.micro");
    assert_eq!(size_of(Cloud::Aws, &wide(3, false))?, "t3.small");
    assert_eq!(size_of(Cloud::Aws, &wide(4, false))?, "m7i-flex.xlarge");
    let got = refusals(&wide(5, false))?;
    let why = got.iter().find(|(c, _)| *c == Cloud::Aws).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert_eq!(
        why,
        "machine `db` is on 5 networks: the largest AWS size here, `m7i-flex.xlarge`, takes 4 network interfaces, one per network"
    );
    Ok(())
}

#[test]
fn the_controller_on_every_network_gets_a_type_with_an_interface_on_each() -> Outcome {
    let aws = cloud_vm(Cloud::Aws, &wide(4, true))?;
    let on_aws = aws.split("resource \"aws_instance\" \"isoloom_controller\"").nth(1).unwrap_or_default();
    assert_eq!(size_in(Cloud::Aws, on_aws)?, "m7i-flex.xlarge");
    let gcp = cloud_vm(Cloud::Gcp, &wide(3, true))?;
    let on_gcp = gcp
        .split("resource \"google_compute_instance\" \"isoloom_controller\"")
        .nth(1)
        .unwrap_or_default();
    assert_eq!(size_in(Cloud::Gcp, on_gcp)?, "e2-standard-4");
    // Three networks for `db` alone, five for the controller only: the controller's own size.
    let mut five = wide(5, true);
    five = five.replace("n3: 10, n4: 10", "n3: 10");
    five = five.replace(
        "db: { os: debian@12, networks: { n0: 10, n1: 10, n2: 10, n3: 10 }",
        "db: { os: debian@12, networks: { n0: 10 }",
    );
    let got = refusals(&five)?;
    let why = got.iter().find(|(c, _)| *c == Cloud::Aws).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert_eq!(
        why,
        "the controller is on 5 networks: the largest AWS size here, `m7i-flex.xlarge`, takes 4 network interfaces, one per network"
    );
    Ok(())
}

#[test]
fn an_aws_type_holds_the_addresses_of_the_busiest_interface() -> Outcome {
    let lab = |octets: &str| {
        format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  db: {{ networks: {{ lan: {octets} }}, os: debian@12, app: {{ steps: [{{ run: s.sh }}] }} }}\n"
        )
    };
    assert_eq!(size_of(Cloud::Aws, &lab("[10, 11, 12]"))?, "t3.small");
    let many: Vec<String> = (10..30).map(|o| o.to_string()).collect();
    let got = refusals(&lab(&format!("[{}]", many.join(", "))))?;
    let why = got.iter().find(|(c, _)| *c == Cloud::Aws).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert_eq!(
        why,
        "machine `db` asks for 20 addresses on one network interface: the largest AWS size here, `m7i-flex.xlarge`, takes 15"
    );
    Ok(())
}

/// Each cloud's refusal of `yaml`.
fn refusals(yaml: &str) -> Outcome<Vec<(Cloud, Option<String>)>> {
    Ok(CloudVmBackend.refusals(&plan(yaml, Target::CloudVm)?))
}

#[test]
fn more_memory_than_a_clouds_largest_size_is_refused() -> Outcome {
    let got = refusals(&lab("32768", ""))?;
    let why = |cloud: Cloud| got.iter().find(|(c, _)| *c == cloud).and_then(|(_, w)| w.clone());
    assert_eq!(
        why(Cloud::DigitalOcean).as_deref(),
        Some("machine `db` asks for 32 GB of memory: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 16 GB")
    );
    for (cloud, size) in [(Cloud::Aws, "`m7i-flex.xlarge`, has 16 GB"), (Cloud::Linode, "`g6-standard-6`, has 16 GB")] {
        let why = why(cloud).unwrap_or_default();
        assert!(
            why.starts_with("machine `db` asks for 32 GB of memory") && why.ends_with(size),
            "{cloud:?}: {why}"
        );
    }
    assert_eq!(why(Cloud::Gcp), None, "Google Cloud's largest has 32 GB");
    assert_eq!(why(Cloud::Azure), None, "Azure's largest has 128 GB");
    assert_eq!(why(Cloud::Oci), None, "Oracle Cloud's shapes are sized to the machine");
    Ok(())
}

#[test]
fn the_largest_size_itself_is_taken() -> Outcome {
    let got = refusals(&lab("16384", ""))?;
    for cloud in [Cloud::Aws, Cloud::Linode] {
        assert!(got.iter().any(|(c, w)| *c == cloud && w.is_none()), "{cloud:?}: {got:?}");
    }
    Ok(())
}

#[test]
fn a_machine_that_nests_is_held_to_the_nesting_sizes() -> Outcome {
    let got = refusals(&lab("65536", "    hardware: { nested_virtualization: true }\n"))?;
    let why = |cloud: Cloud| got.iter().find(|(c, _)| *c == cloud).and_then(|(_, w)| w.clone()).unwrap_or_default();
    assert!(why(Cloud::Aws).ends_with("`m8i.2xlarge`, has 32 GB"), "{}", why(Cloud::Aws));
    assert!(why(Cloud::Gcp).ends_with("`n2-standard-8`, has 32 GB"), "{}", why(Cloud::Gcp));
    Ok(())
}
