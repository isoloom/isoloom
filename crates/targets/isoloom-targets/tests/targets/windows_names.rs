//! A Windows machine's name is its computer name, which NetBIOS keeps to 15 characters: every
//! target that makes a Windows VM refuses a longer one, the clouds included, rather than cutting
//! two names into one.

use isoloom_spec::Target;
use isoloom_targets::{Error, GenerateError, generate};

use crate::testing::{Outcome, lab};

#[test]
fn every_vm_target_refuses_two_long_windows_names_the_clouds_included() -> Outcome {
    let long = lab(
        "version: 1\nname: win\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  windows-server-01: { networks: { lan: 10 }, os: windows-server@2022 }\n  windows-server-02: { networks: { lan: 11 }, os: windows-server@2022 }\n",
    )?;
    let why = "machine `windows-server-01`: Windows computer names are at most 15 characters";
    for target in [Target::Vagrant, Target::Proxmox, Target::Esxi, Target::CloudVm] {
        let refused = isoloom_targets::refusal(&long, target).unwrap_or_default();
        assert!(refused.contains(why), "{target:?}: {refused}");
        assert!(
            matches!(generate(&long, target), Err(Error::Generate(GenerateError::Unsupported { .. }))),
            "{target:?} generates"
        );
    }
    Ok(())
}
