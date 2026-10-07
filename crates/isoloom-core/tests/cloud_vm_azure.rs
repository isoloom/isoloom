//! The `cloud-vm` Azure driver: a faithful port of the AWS one to the azurerm provider. These
//! assert the Azure-specific facts; `generate.rs` checks the committed output stays in sync.

use std::path::Path;

use isoloom_core::{Spec, Target, generate, load};

fn example(name: &str) -> Spec {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    load(&dir).expect("example parses")
}

fn azure(spec: &Spec) -> String {
    generate(spec, Target::CloudVm)
        .unwrap()
        .into_iter()
        .find(|f| f.path == ".isoloom/cloud-vm/azure/main.tf")
        .expect("azure main.tf generated")
        .contents
}

#[test]
fn azure_uses_azurerm_with_static_addresses_and_the_shared_outputs() {
    let tf = azure(&example("hello-stack"));
    // The azurerm provider and an Azure VM, not an AWS instance.
    assert!(tf.contains("source  = \"hashicorp/azurerm\""), "{tf}");
    assert!(tf.contains("version = \"~> 5.0\""), "{tf}");
    assert!(tf.contains("resource \"azurerm_linux_virtual_machine\" \"web\""));
    // The launcher contract: a `region` var and ARM_SUBSCRIPTION_ID from the environment.
    assert!(tf.contains("variable \"region\""), "{tf}");
    assert!(tf.contains("subscription_id = var.subscription_id"), "{tf}");
    assert!(tf.contains("location            = var.region"), "{tf}");
    assert!(!tf.contains("var.location"), "{tf}");
    // Every address of the spec is kept: a static private IP at the spec's octet.
    assert!(tf.contains("private_ip_address_allocation = \"Static\""));
    assert!(tf.contains("private_ip_address            = \"10.60.0.10\""), "{tf}");
    // A static public IP the launcher reaches it on.
    assert!(tf.contains("resource \"azurerm_public_ip\" \"web\""));
    assert!(tf.contains("allocation_method   = \"Static\""));
    // The outputs the launcher consumes, identical in name and shape to AWS.
    assert!(tf.contains("output \"machines\""));
    assert!(tf.contains("output \"ssh_users\""));
    assert!(tf.contains("output \"ip\""));
    assert!(tf.contains("output \"ready_file\""));
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""));
    assert!(tf.contains("azurerm_public_ip.web.ip_address"));
}

#[test]
fn azure_gives_a_machine_on_several_networks_an_interface_on_each() {
    let mut spec = example("pivot-dmz");
    // Kali has no Azure image yet: the user lands on Debian here (as the AWS test does).
    for m in spec.machines.values_mut() {
        if let Some(vm) = &mut m.vm
            && vm.os == "kali"
        {
            vm.os = "debian-12".into();
        }
    }
    let tf = azure(&spec);
    // The multi-homed gateway gets a NIC per network, forwarding on (AWS source/dest check off).
    assert!(tf.contains("resource \"azurerm_network_interface\" \"gateway_dmz\""), "{tf}");
    assert!(tf.contains("resource \"azurerm_network_interface\" \"gateway_internal\""));
    assert!(tf.contains("ip_forwarding_enabled = true"));
    // Its extra interface is found by MAC: Azure reports it dash-separated and uppercase, so it
    // is lowered and colon-joined, and interpolated into the set-up (not left a literal). Inside
    // the interpolation the quotes are HCL's own, not escaped (terraform rejects `\"` there).
    assert!(
        tf.contains("${lower(replace(azurerm_network_interface.gateway_internal.mac_address, \"-\", \":\"))}"),
        "{tf}"
    );
    assert!(!tf.contains("$${lower("));
}

#[test]
fn azure_runs_windows_over_winrm_and_a_controller_runs_the_checks() {
    let tf = azure(&example("windows-hello"));
    // A Windows VM with the generated password, WinRM over HTTP.
    assert!(tf.contains("resource \"azurerm_windows_virtual_machine\" \"web01\""), "{tf}");
    assert!(tf.contains("resource \"random_password\" \"windows\""));
    assert!(tf.contains("protocol = \"Http\""));
    assert!(tf.contains("type     = \"winrm\""));
    // A Windows-only environment: a Debian controller runs the checks from its public IP.
    assert!(tf.contains("resource \"azurerm_linux_virtual_machine\" \"isoloom_controller\""));
    assert!(tf.contains("host = azurerm_public_ip.isoloom_controller.ip_address"));
}
