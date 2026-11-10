//! Every server module checks its `slot` against the range its description gives: Proxmox's (and
//! `docker-vm`'s on Proxmox) said "1 to 99" and took any number, which reached the SDN's ids and
//! the router's MACs and failed only at apply time.

use isoloom_spec::model::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, between, example};

#[test]
fn every_server_module_checks_its_slot_against_the_range_it_describes() -> Outcome {
    let lab = example("hello-stack")?;
    let mut modules = 0;
    for target in [Target::Proxmox, Target::Esxi, Target::DockerVm] {
        for f in generate(&lab, target)?.iter().filter(|f| f.path.ends_with("main.tf")) {
            let Ok(slot) = between(&f.contents, "variable \"slot\" {", "\n}\n") else {
                continue;
            };
            let described = between(slot, "description = \"1 to ", ",")?;
            let max: u64 = described.trim_start_matches("description = \"1 to ").parse()?;
            assert!(
                slot.contains(&format!("condition     = var.slot >= 1 && var.slot <= {max}")),
                "{}: {slot}",
                f.path
            );
            assert!(slot.contains(&format!("error_message = \"slot is 1 to {max}.\"")), "{}: {slot}", f.path);
            modules += 1;
        }
    }
    assert_eq!(modules, 3, "proxmox, esxi and docker-vm's proxmox module each have a slot");
    Ok(())
}
