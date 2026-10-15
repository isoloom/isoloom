//! A machine's GPUs on Proxmox: each passed through to its VM as a PCI device (`hostpci`), by
//! the server's PCI address (`0000:01:00.0`, which needs the `root@pam` user) or by a resource
//! mapping's name (which an API token may use). The VM is a q35 machine, so the GPU is a PCIe
//! device. Proxmox can't pick GPUs by count: the spec names them (`gpus.ids`).

use isoloom_plan::{Environment, MachineShape};
use isoloom_render::hcl_builder::{Block, boolean, string};
use isoloom_spec::model::{GpuCount, Gpus};

/// Why Proxmox can't pass a machine's GPUs through, if it can't.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .find_map(|(name, mp)| mp.hardware.gpus.as_ref().and_then(|g| why(name, g)))
}

fn why(name: &str, g: &Gpus) -> Option<String> {
    let at = format!("machine `{name}`");
    if g.count == Some(GpuCount::All) || g.ids.is_empty() {
        return Some(format!(
            "{at}: Proxmox passes the server's GPUs by name: list them in `gpus.ids` (PCI addresses such as 0000:01:00.0, or resource mappings)"
        ));
    }
    (!g.options.is_empty()).then(|| format!("{at}: `gpus.options` are Docker's GPU driver options, which a Proxmox VM has no place for"))
}

/// Whether `id` is a PCI address (`0000:01:00.0`, `01:00.0`, `01:00` for every function).
fn pci_address(id: &str) -> bool {
    let hex = |s: &str, n: usize| s.len() == n && s.chars().all(|c| c.is_ascii_hexdigit());
    let (slot, function) = id.rsplit_once('.').map_or((id, None), |(s, f)| (s, Some(f)));
    let parts: Vec<&str> = slot.split(':').collect();
    let slot_ok = match parts.as_slice() {
        [domain, bus, dev] => hex(domain, 4) && hex(bus, 2) && hex(dev, 2),
        [bus, dev] => hex(bus, 2) && hex(dev, 2),
        _ => false,
    };
    slot_ok && function.is_none_or(|f| hex(f, 1))
}

/// The VM's `hostpci` blocks, one per GPU, as PCIe devices.
pub(super) fn hostpci(gpus: Option<&Gpus>) -> Vec<Block> {
    let Some(g) = gpus else { return Vec::new() };
    g.ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let source = if pci_address(id) { "id" } else { "mapping" };
            Block::builder("hostpci")
                .add_attribute(("device", string(format!("hostpci{i}"))))
                .add_attribute((source, string(id.as_str())))
                .add_attribute(("pcie", boolean(true)))
                .build()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    fn gpus(v: serde_json::Value) -> Gpus {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn gpus_are_passed_by_name_only() {
        assert!(why("a", &gpus(json!(1))).is_some_and(|w| w.contains("`gpus.ids`")));
        assert!(why("a", &gpus(json!("all"))).is_some());
        assert!(why("a", &gpus(json!({ "ids": ["gpu0"], "options": { "a": "b" } }))).is_some_and(|w| w.contains("`gpus.options`")));
        assert_eq!(why("a", &gpus(json!({ "ids": ["gpu0"], "capabilities": ["compute"] }))), None);
    }

    #[test]
    fn a_pci_address_is_an_id_and_anything_else_a_mapping() {
        for id in ["0000:01:00.0", "01:00.0", "01:00", "0000:af:00"] {
            assert!(pci_address(id), "{id}");
        }
        for id in ["gpu0", "01:00.10", "1:00.0", "0000:01:00:00"] {
            assert!(!pci_address(id), "{id}");
        }
        let mut m = isoloom_render::hcl_builder::Module::new("");
        m.blocks(hostpci(Some(&gpus(json!({ "ids": ["0000:01:00.0", "rtx"] })))));
        let text = m.render().unwrap();
        assert!(
            text.contains("hostpci {\n  device = \"hostpci0\"\n  id     = \"0000:01:00.0\"\n  pcie   = true\n}"),
            "{text}"
        );
        assert!(
            text.contains("hostpci {\n  device  = \"hostpci1\"\n  mapping = \"rtx\"\n  pcie    = true\n}"),
            "{text}"
        );
    }

    proptest! {
        #[test]
        fn every_pci_address_written_out_is_one(domain: u16, bus: u8, device in 0u8..32, function in 0u8..8) {
            let slot = format!("{bus:02x}:{device:02x}");
            for id in [format!("{domain:04x}:{slot}.{function}"), format!("{slot}.{function}"), slot.clone()] {
                prop_assert!(pci_address(&id), "{id}");
            }
        }

        #[test]
        fn a_mapping_name_is_no_pci_address(name in "[g-z][a-z0-9_-]{0,12}") {
            prop_assert!(!pci_address(&name));
        }
    }
}
