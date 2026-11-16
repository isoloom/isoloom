//! A machine's size on the clouds whose sizes come from a table: the smallest with its CPUs,
//! memory, disk (where the size fixes it) and an interface per network; more than the largest
//! has is refused, naming the machine and that size, never silently given less. Oracle Cloud's
//! flexible shape is sized to the machine.

use crate::testing::{Outcome, cloud_vm, plan};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};
use isoloom_test_support::{hcl_nested, hcl_resource, hcl_text};

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

/// The size `cloud`'s module gives VM `name` (`db`, `isoloom_controller`) for `yaml`: its
/// instance's type, machine type, size or plan, read from the parsed resource.
fn size_of(cloud: Cloud, yaml: &str, name: &str) -> Outcome<String> {
    let (kind, key) = match cloud {
        Cloud::Aws => ("aws_instance", "instance_type"),
        Cloud::Gcp => ("google_compute_instance", "machine_type"),
        Cloud::Azure => ("azurerm_linux_virtual_machine", "size"),
        Cloud::DigitalOcean => ("digitalocean_droplet", "size"),
        Cloud::Linode => ("linode_instance", "type"),
        _ => return Err(format!("{cloud}'s shapes are sized, not picked").into()),
    };
    Ok(hcl_text(&hcl_resource(&cloud_vm(cloud, yaml)?, kind, name), key))
}

/// The OCPUs and memory (GB) of `db`'s flexible Oracle Cloud shape for `yaml`.
fn oci_shape(yaml: &str) -> Outcome<(String, String)> {
    let db = hcl_resource(&cloud_vm(Cloud::Oci, yaml)?, "oci_core_instance", "db");
    let shape = hcl_nested(&db, "shape_config");
    let shape = shape.first().ok_or("no shape_config")?.body();
    Ok((hcl_text(shape, "ocpus"), hcl_text(shape, "memory_in_gbs")))
}

/// Why `cloud` refuses `yaml`'s environment, if it does.
fn refusal(cloud: Cloud, yaml: &str) -> Outcome<Option<String>> {
    Ok(CloudVmModule::new(cloud).refusal(&plan(yaml, Target::CloudVm)?))
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
        assert_eq!(size_of(cloud, &four, "db")?, size, "{cloud:?}");
    }
    // An AMD OCPU is two vCPUs, and the shape takes 1 GB an OCPU at least.
    assert_eq!(oci_shape(&sized("cpus: 8, memory_mb: 1024", ""))?, ("4".to_owned(), "4".to_owned()));
    assert_eq!(oci_shape(&sized("cpus: 1, memory_mb: 16384", ""))?, ("2".to_owned(), "16".to_owned()));
    Ok(())
}

#[test]
fn more_cpus_than_a_clouds_largest_size_is_refused() -> Outcome {
    let yaml = sized("cpus: 64", "");
    for (cloud, size) in [
        (Cloud::Aws, "`m7i-flex.xlarge`, has 4 vCPUs"),
        (Cloud::Gcp, "`e2-standard-8`, has 8 vCPUs"),
        (Cloud::Azure, "`Standard_D32as_v5`, has 32 vCPUs"),
        (Cloud::DigitalOcean, "`s-8vcpu-16gb`, has 8 vCPUs"),
        (Cloud::Linode, "`g6-standard-6`, has 6 vCPUs"),
    ] {
        let why = refusal(cloud, &yaml)?.ok_or(format!("{cloud} takes 64 CPUs"))?;
        assert!(why.starts_with("machine `db` asks for 64 CPUs") && why.ends_with(size), "{cloud:?}: {why}");
    }
    assert_eq!(refusal(Cloud::Oci, &yaml)?, None, "64 CPUs are 32 OCPUs");
    let oci = refusal(Cloud::Oci, &sized("cpus: 200", ""))?.ok_or("Oracle Cloud takes 200 CPUs")?;
    assert!(oci.contains("machine `db` asks for 100 OCPUs") && oci.contains("64 OCPUs (128 CPUs)"), "{oci}");
    Ok(())
}

#[test]
fn a_disk_picks_the_size_that_comes_with_as_much_or_is_refused() -> Outcome {
    let big = sized("disk_gb: 100", "");
    assert_eq!(size_of(Cloud::DigitalOcean, &big, "db")?, "s-4vcpu-8gb");
    assert_eq!(size_of(Cloud::Linode, &big, "db")?, "g6-standard-4");
    let huge = sized("disk_gb: 400", "");
    assert_eq!(
        refusal(Cloud::DigitalOcean, &huge)?.as_deref(),
        Some("machine `db` asks for a 400 GB disk: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 320 GB")
    );
    let linode = refusal(Cloud::Linode, &huge)?.ok_or("Linode takes 400 GB")?;
    assert!(linode.ends_with("`g6-standard-6`, has 320 GB"), "{linode}");
    assert_eq!(refusal(Cloud::Aws, &huge)?, None, "an AWS disk is the machine's own");
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
    for (networks, size) in [(2, "t3.micro"), (3, "t3.small"), (4, "m7i-flex.xlarge")] {
        assert_eq!(size_of(Cloud::Aws, &wide(networks, false), "db")?, size, "{networks} networks");
    }
    assert_eq!(
        refusal(Cloud::Aws, &wide(5, false))?.as_deref(),
        Some("machine `db` is on 5 networks: the largest AWS size here, `m7i-flex.xlarge`, takes 4 network interfaces, one per network")
    );
    Ok(())
}

#[test]
fn the_controller_on_every_network_gets_a_type_with_an_interface_on_each() -> Outcome {
    assert_eq!(size_of(Cloud::Aws, &wide(4, true), "isoloom_controller")?, "m7i-flex.xlarge");
    assert_eq!(size_of(Cloud::Gcp, &wide(3, true), "isoloom_controller")?, "e2-standard-4");
    // Three networks for `db` alone, five for the controller only: the controller's own size.
    let mut five = wide(5, true);
    five = five.replace("n3: 10, n4: 10", "n3: 10");
    five = five.replace(
        "db: { os: debian@12, networks: { n0: 10, n1: 10, n2: 10, n3: 10 }",
        "db: { os: debian@12, networks: { n0: 10 }",
    );
    assert_eq!(
        refusal(Cloud::Aws, &five)?.as_deref(),
        Some("the controller is on 5 networks: the largest AWS size here, `m7i-flex.xlarge`, takes 4 network interfaces, one per network")
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
    assert_eq!(size_of(Cloud::Aws, &lab("[10, 11, 12]"), "db")?, "t3.small");
    let many: Vec<String> = (10..30).map(|o| o.to_string()).collect();
    assert_eq!(
        refusal(Cloud::Aws, &lab(&format!("[{}]", many.join(", "))))?.as_deref(),
        Some("machine `db` asks for 20 addresses on one network interface: the largest AWS size here, `m7i-flex.xlarge`, takes 15")
    );
    Ok(())
}

#[test]
fn more_memory_than_a_clouds_largest_size_is_refused() -> Outcome {
    let yaml = lab("32768", "");
    assert_eq!(
        refusal(Cloud::DigitalOcean, &yaml)?.as_deref(),
        Some("machine `db` asks for 32 GB of memory: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 16 GB")
    );
    for (cloud, size) in [(Cloud::Aws, "`m7i-flex.xlarge`, has 16 GB"), (Cloud::Linode, "`g6-standard-6`, has 16 GB")] {
        let why = refusal(cloud, &yaml)?.ok_or(format!("{cloud} takes 32 GB"))?;
        assert!(
            why.starts_with("machine `db` asks for 32 GB of memory") && why.ends_with(size),
            "{cloud:?}: {why}"
        );
    }
    assert_eq!(refusal(Cloud::Gcp, &yaml)?, None, "Google Cloud's largest has 32 GB");
    assert_eq!(refusal(Cloud::Azure, &yaml)?, None, "Azure's largest has 128 GB");
    assert_eq!(refusal(Cloud::Oci, &yaml)?, None, "Oracle Cloud's shapes are sized to the machine");
    // The largest size itself is taken.
    for cloud in [Cloud::Aws, Cloud::Linode] {
        assert_eq!(refusal(cloud, &lab("16384", ""))?, None, "{cloud:?}");
    }
    Ok(())
}

#[test]
fn a_machine_that_nests_is_held_to_the_nesting_sizes() -> Outcome {
    let yaml = lab("65536", "    hardware: { nested_virtualization: true }\n");
    for (cloud, largest) in [(Cloud::Aws, "`m8i.2xlarge`, has 32 GB"), (Cloud::Gcp, "`n2-standard-8`, has 32 GB")] {
        let why = refusal(cloud, &yaml)?.ok_or(format!("{cloud} takes 64 GB nested"))?;
        assert!(why.ends_with(largest), "{cloud:?}: {why}");
    }
    Ok(())
}
