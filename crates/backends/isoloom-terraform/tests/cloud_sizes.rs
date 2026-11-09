//! A machine's memory on the clouds whose sizes come from a table: more than the largest is
//! refused, naming the machine and that size, never silently given less.

pub mod testing;

use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmBackend};
use testing::{Outcome, plan};

/// One Linux machine with `memory` and `extra` in its spec.
fn lab(memory: &str, extra: &str) -> String {
    format!(
        "version: 1\nname: big\nnetworks: {{ lab: {{ cidr: 10.60.0.0/24 }} }}\nmachines:\n  db:\n    os: debian@12\n    networks: {{ lab: 10 }}\n    resources: {{ memory_mb: {memory} }}\n{extra}    app: {{ steps: [{{ run: p.sh }}] }}\n"
    )
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
        Some("machine `db` asks for 32 GB of memory: the largest DigitalOcean size here, `s-4vcpu-8gb`, has 8 GB")
    );
    for (cloud, size) in [
        (Cloud::Aws, "`m7i-flex.xlarge`, has 16 GB"),
        (Cloud::Gcp, "`e2-standard-4`, has 16 GB"),
        (Cloud::Linode, "`g6-standard-6`, has 16 GB"),
    ] {
        let why = why(cloud).unwrap_or_default();
        assert!(
            why.starts_with("machine `db` asks for 32 GB of memory") && why.ends_with(size),
            "{cloud:?}: {why}"
        );
    }
    assert_eq!(why(Cloud::Azure), None, "Azure's largest has 128 GB");
    assert_eq!(why(Cloud::Oci), None, "Oracle Cloud's shapes are sized to the machine");
    Ok(())
}

#[test]
fn the_largest_size_itself_is_taken() -> Outcome {
    let got = refusals(&lab("16384", ""))?;
    for cloud in [Cloud::Aws, Cloud::Gcp, Cloud::Linode] {
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
