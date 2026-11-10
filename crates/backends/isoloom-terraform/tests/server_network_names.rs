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
    let env = planned(&example(name)?, backend.target())?;
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

/// The router's NIC on the network at `index` of the plan (its MAC says which), and the VNet
/// resource its `bridge` points at.
fn router_vnet(module: &str, index: usize) -> Option<&str> {
    let mac = format!("format(\"02:15:%02x:01:%02x:00\", var.slot, {index})");
    let device = module
        .split("network_device {")
        .find(|d| d.split('}').next().is_some_and(|d| d.contains(&mac)))?;
    attribute(device, "bridge")?.strip_prefix("proxmox_sdn_vnet.")?.strip_suffix(".id")
}

#[test]
fn every_proxmox_network_is_named_after_the_vnet_it_is_on() -> Outcome {
    for name in ["hello-stack", "segmented", "vlan-office"] {
        let env = planned(&example(name)?, ProxmoxBackend.target())?;
        let (module, names) = natives(&ProxmoxBackend, name)?;
        for (net, native) in names {
            assert!(
                native.starts_with("i${var.slot}") && native.len() > "i${var.slot}".len(),
                "{name}: {net} is {native}"
            );
            // The VNet the router's NIC on this network is bridged to.
            let index = env.networks.get_index_of(&net).ok_or_else(|| format!("{name}: no network {net}"))?;
            let vnet = router_vnet(&module, index).ok_or_else(|| format!("{name}: the router has no NIC on {net}"))?;
            let body = resource_body(&module, &format!("resource \"proxmox_sdn_vnet\" \"{vnet}\""));
            assert_eq!(attribute(body, "id"), Some(native.as_str()), "{name}: {net} is on VNet {vnet}: {body}");
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
