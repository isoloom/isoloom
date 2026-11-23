//! Every example's Vagrantfile reads as Ruby blocks, each VM defined once, its machines in the
//! spec's start order: what the other Vagrant tests read their assertions from.

use isoloom_spec::Target;

use crate::testing::{Outcome, defined, example, examples, vagrantfile};

#[test]
fn every_examples_vagrantfile_reads_as_blocks_with_each_vm_defined_once() -> Outcome {
    let mut read = 0;
    for dir in std::fs::read_dir(examples())? {
        let dir = dir?.path();
        let Some(name) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if !dir.join("isoloom.yml").is_file() {
            continue;
        }
        let lab = example(&name)?;
        if !lab.possible().contains(&Target::Vagrant) {
            continue;
        }
        let vms = defined(&vagrantfile(&lab)?);
        let env = lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?;
        for machine in env.machines.keys() {
            assert_eq!(vms.iter().filter(|v| *v == machine).count(), 1, "{name}: `{machine}` in {vms:?}");
        }
        read += 1;
    }
    assert!(read > 10, "only {read} examples generate for Vagrant");
    Ok(())
}
