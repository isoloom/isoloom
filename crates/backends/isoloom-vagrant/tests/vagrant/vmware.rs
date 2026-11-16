//! VMware Desktop's part of a Vagrantfile.

use isoloom_test_support::assert_every;

use crate::testing::{Outcome, vagrantfile};

/// On VMware Desktop each adapter on the environment's networks is pinned to a PCI slot of its
/// own (`ethernet<n>.pcislotnumber`, from 1: the box's NAT adapter keeps its own), one per
/// `private_network` line of the VM: a slot VMware picked as the adapter was added moved the
/// guest's interfaces under other names, and closed its SSH session.
#[test]
fn every_vmware_adapter_on_a_network_has_a_pci_slot_of_its_own() -> Outcome {
    for example in ["segmented", "supplier-portal-api", "dmz-jump"] {
        let file = vagrantfile(example)?;
        let r = isoloom_test_support::ruby_blocks(&file);
        for vm in r.find("config.vm.define ") {
            let name = vm.head.split('"').nth(1).unwrap_or_default();
            let networks = vm.statements("m.vm.network \"private_network\"").len();
            let vmware = vm.find("m.vm.provider \"vmware_desktop\" do |");
            let vmware = vmware.first().ok_or_else(|| format!("{example}: {name} has no VMware block"))?;
            let slots: Vec<(String, String)> = vmware
                .statements("v.vmx[\"ethernet")
                .iter()
                .filter_map(|s| s.strip_prefix("v.vmx[\"ethernet")?.split_once(".pcislotnumber\"] = "))
                .map(|(n, slot)| (n.to_owned(), slot.trim_matches('"').to_owned()))
                .collect();
            let adapters: Vec<String> = slots.iter().map(|(n, _)| n.clone()).collect();
            let wanted: Vec<String> = (1..=networks).map(|n| n.to_string()).collect();
            assert_eq!(adapters, wanted, "{example}: {name}'s adapters");
            let distinct: std::collections::BTreeSet<&str> = slots.iter().map(|(_, s)| s.as_str()).collect();
            assert_eq!(distinct.len(), slots.len(), "{example}: {name}: {slots:?}");
            assert_every!(&slots, |(_, s): &&(String, String)| s.parse::<u32>().is_ok_and(|n| n >= 160));
        }
    }
    Ok(())
}
