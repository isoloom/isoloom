//! The controller's inventory holds the Windows machines' password, so every target that writes
//! it (a cloud's user data, Proxmox's cloud-init, ESXi's upload) makes it owner-only, as the key.

use crate::testing::{Outcome, WINDOWS_PROVISIONED, file, has_line, line_with, plan, texts, written, written_files};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, EsxiBackend, ProxmoxBackend};

#[test]
fn a_clouds_controller_writes_its_inventory_owner_only() -> Outcome {
    let env = plan(WINDOWS_PROVISIONED, Target::CloudVm)?;
    let tf = CloudVmModule::new(Cloud::Aws).render(&env)?.contents;
    let setup = texts(&tf, "aws_instance", "isoloom_controller");
    let line = line_with(&setup, "/etc/isoloom/inventory.ini")?;
    assert!(
        line.contains("base64 -d > /etc/isoloom/inventory.ini) && chmod 0600 /etc/isoloom/inventory.ini"),
        "{line}"
    );
    Ok(())
}

#[test]
fn proxmoxs_controller_writes_its_inventory_owner_only() -> Outcome {
    let env = plan(WINDOWS_PROVISIONED, Target::Proxmox)?;
    let tf = file(&ProxmoxBackend.render(&env)?, "proxmox/main.tf")?.to_owned();
    let files = written_files(&tf, "proxmox_virtual_environment_file", "isoloom_controller");
    let inventory = written(&files, "/etc/isoloom/inventory.ini")?;
    assert_eq!(inventory.get("permissions").map(String::as_str), Some("0600"));
    Ok(())
}

#[test]
fn esxis_controller_installs_its_inventory_owner_only() -> Outcome {
    let env = plan(WINDOWS_PROVISIONED, Target::Esxi)?;
    let tf = file(&EsxiBackend.render(&env)?, "esxi/main.tf")?.to_owned();
    let setup = texts(&tf, "terraform_data", "isoloom_controller");
    assert!(
        has_line(&setup, "install -m 0600 /tmp/isoloom-inventory.ini /etc/isoloom/inventory.ini"),
        "{setup:?}"
    );
    Ok(())
}
