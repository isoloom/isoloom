//! The controller's inventory holds the Windows machines' password, so every target that writes
//! it (a cloud's user data, Proxmox's cloud-init, ESXi's upload) makes it owner-only, as the key.

pub mod testing;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, EsxiBackend, ProxmoxBackend};
use testing::{Outcome, file, plan};

/// A Windows machine the controller provisions.
const LAB: &str = "version: 1
name: t
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  dc: { networks: { lan: 10 }, os: windows-server@2022 }
provision:
  - ansible: provision/site.yml
";

#[test]
fn a_clouds_controller_writes_its_inventory_owner_only() -> Outcome {
    let env = plan(LAB, Target::CloudVm)?;
    let tf = CloudVmModule::new(Cloud::Aws).render(&env)?.contents;
    assert!(
        tf.contains("base64 -d > /etc/isoloom/inventory.ini) && chmod 0600 /etc/isoloom/inventory.ini"),
        "the inventory is owner-only: {tf}"
    );
    Ok(())
}

#[test]
fn proxmoxs_controller_writes_its_inventory_owner_only() -> Outcome {
    let env = plan(LAB, Target::Proxmox)?;
    let tf = file(&ProxmoxBackend.render(&env)?, "proxmox/main.tf")?.to_owned();
    let at = tf.find("/etc/isoloom/inventory.ini").ok_or("no inventory in the module")?;
    let entry = &tf[at.saturating_sub(200)..(at + 200).min(tf.len())];
    assert!(entry.contains("\"0600\""), "the inventory is owner-only: {entry}");
    assert!(!entry.contains("\"0644\""), "{entry}");
    Ok(())
}

#[test]
fn esxis_controller_installs_its_inventory_owner_only() -> Outcome {
    let env = plan(LAB, Target::Esxi)?;
    let tf = file(&EsxiBackend.render(&env)?, "esxi/main.tf")?.to_owned();
    assert!(
        tf.contains("install -m 0600 /tmp/isoloom-inventory.ini /etc/isoloom/inventory.ini"),
        "the inventory is owner-only: {tf}"
    );
    Ok(())
}
