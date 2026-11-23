//! Every server module checks its `slot` against the range its description gives: Proxmox's (and
//! `docker-vm`'s on Proxmox) said "1 to 99" and took any number, which reached the SDN's ids and
//! the router's MACs and failed only at apply time.

use isoloom_spec::model::Target;
use isoloom_targets::generate;

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_nested, hcl_text};

use crate::testing::{Outcome, example};

#[test]
fn every_server_module_checks_its_slot_against_the_range_it_describes() -> Outcome {
    let lab = example("hello-stack")?;
    let mut modules = 0;
    for target in [Target::Proxmox, Target::Esxi] {
        for f in generate(&lab, target)?.iter().filter(|f| f.path.ends_with("main.tf")) {
            let body = hcl_body(&f.contents);
            let Some(slot) = hcl_blocks(&body, "variable", &["slot"]).first().map(|b| b.body().clone()) else {
                continue;
            };
            let description = hcl_text(&slot, "description");
            let max: u64 = description
                .strip_prefix("1 to ")
                .and_then(|r| r.split(',').next())
                .ok_or_else(|| format!("{}: {description}", f.path))?
                .parse()?;
            let validation = hcl_nested(&slot, "validation");
            let validation = validation.first().ok_or("no validation")?.body();
            assert_eq!(hcl_text(validation, "condition"), format!("var.slot >= 1 && var.slot <= {max}"), "{}", f.path);
            assert_eq!(hcl_text(validation, "error_message"), format!("slot is 1 to {max}."), "{}", f.path);
            modules += 1;
        }
    }
    assert_eq!(modules, 2, "proxmox and esxi each have a slot");
    Ok(())
}
