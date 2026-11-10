//! A server's run manifest names each network as the module makes it: ESXi's port group, the
//! Proxmox VNet a network is on (its LAN's, for a VLAN). The module writes the name after
//! `var.slot`, the run manifest after `${run.slot}`: one with the slot put back is the other.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_terraform::{EsxiBackend, ProxmoxBackend};
use testing::{Outcome, example, file, planned, resource_body};

/// Each network's native name in `backend`'s run manifest for example `name`, with the module's
/// slot (`${var.slot}`) in place of the run's.
fn natives(backend: &dyn Backend, name: &str) -> Outcome<(String, Vec<(String, String)>)> {
    let env = planned(&example(name)?, backend.target());
    let module = file(&backend.render(&env)?, &format!("{}/main.tf", backend.target().id()))?.to_owned();
    let manifest = backend.run_manifest(&env)?.ok_or("a run manifest")?;
    assert_eq!(manifest.networks.len(), env.networks.len(), "every network named");
    let names = manifest
        .networks
        .iter()
        .map(|n| {
            let native = n.native.as_ref().map(ToString::to_string).unwrap_or_default();
            (n.name.to_string(), native.replace("${run.slot}", "${var.slot}"))
        })
        .collect();
    Ok((module, names))
}

/// The quoted value of the first `key = "..."` line in `body`, whatever its alignment.
fn attribute<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"'))
    })
}

#[test]
fn every_esxi_network_is_named_after_its_port_group() -> Outcome {
    for example in ["hello-stack", "segmented", "vlan-office"] {
        let (module, names) = natives(&EsxiBackend, example)?;
        for (net, native) in names {
            let group = resource_body(&module, &format!("resource \"esxi_portgroup\" \"{}\"", net.replace('-', "_")));
            assert_eq!(attribute(group, "name"), Some(native.as_str()), "{example}: {net}'s port group: {group}");
        }
    }
    Ok(())
}

#[test]
fn every_proxmox_network_is_named_after_the_vnet_it_is_on() -> Outcome {
    for example in ["hello-stack", "segmented", "vlan-office"] {
        let (module, names) = natives(&ProxmoxBackend, example)?;
        for (net, native) in names {
            assert!(
                native.starts_with("i${var.slot}") && native.len() > "i${var.slot}".len(),
                "{example}: {net} is {native}"
            );
            let vnets: Vec<&str> = module
                .split("resource \"proxmox_sdn_vnet\" ")
                .skip(1)
                .filter_map(|b| attribute(b, "id"))
                .collect();
            assert!(vnets.contains(&native.as_str()), "{example}: {net} is {native}, which no VNet is: {vnets:?}");
        }
    }
    Ok(())
}

#[test]
fn a_vlan_is_named_after_its_lans_vnet() -> Outcome {
    let (_, names) = natives(&ProxmoxBackend, "vlan-office")?;
    let lans: Vec<&str> = names.iter().map(|(_, n)| n.as_str()).collect();
    assert!(
        lans.len() > 1 && lans.windows(2).any(|w| w[0] == w[1]),
        "VLANs share their LAN's VNet: {names:?}"
    );
    Ok(())
}
