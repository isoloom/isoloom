//! A Windows machine in the cloud gets no secrets: its set-up runs over WinRM during Terraform's
//! apply, and the runner delivers a machine's secrets once `up`'s steps are done, so the set-up
//! would wait for them until it timed out. AWS and Azure refuse it; a Windows machine without
//! secrets still generates.

use isoloom_spec::Target;
use isoloom_targets::refusal_on;

use crate::testing::{Outcome, lab};

const SPEC: &str = "version: 1\nname: w\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  dc: { os: windows-server@2022, networks: { lan: 10 }FILES }\n";

#[test]
fn a_windows_machine_in_the_cloud_with_secrets_is_refused() -> Outcome {
    let with = lab(&format!(
        "secrets: {{ db: {{ env: DB }} }}\n{}",
        SPEC.replace("FILES", ", files: [{ path: 'C:\\isoloom\\db.txt', secret: db }]")
    ))?;
    let without = lab(&SPEC.replace("FILES", ""))?;
    for cloud in ["aws", "azure"] {
        let why = refusal_on(&with, Target::CloudVm, Some(cloud)).ok_or_else(|| format!("{cloud} refuses it"))?;
        assert!(
            why.contains("machine `dc` gets `secrets`") && why.contains("before the runner delivers them"),
            "{cloud}: {why}"
        );
        assert_eq!(refusal_on(&without, Target::CloudVm, Some(cloud)), None, "{cloud}");
    }
    Ok(())
}
