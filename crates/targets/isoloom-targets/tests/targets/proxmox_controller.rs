//! Environment-level `provision:` on Proxmox: a controller VM runs the playbooks, as on Vagrant
//! and the clouds. Before this, Proxmox refused such environments outright.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{hcl, hcl_attribute, hcl_body, hcl_names, hcl_nested, hcl_one, hcl_text};

use crate::testing::{Outcome, example};

#[test]
fn proxmox_runs_environment_playbooks_from_a_controller() -> Outcome {
    let spec = example("ansible-pair")?;
    let files = generate(&spec, Target::Proxmox).expect("environment provisioning is no longer refused on Proxmox");
    let tf = &files[0].contents;
    let body = hcl_body(tf);
    // Its own key, given to the `isoloom` user on every machine alongside the operator's.
    assert_eq!(hcl_names(&body, "resource", "tls_private_key"), ["controller"]);
    assert!(tf.contains("trimspace(tls_private_key.controller.public_key_openssh)"), "the key handed on");
    // The controller VM: on the uplink (its route out, to install Ansible) and on every network.
    let controller = hcl_one(&body, "proxmox_virtual_environment_vm", "isoloom_controller");
    let bridges: Vec<String> = hcl_nested(controller, "network_device").iter().map(|d| hcl_text(d.body(), "bridge")).collect();
    assert_eq!(bridges.first().map(String::as_str), Some("var.uplink_bridge"), "{bridges:?}");
    assert!(bridges.len() > 1, "and a card on every network: {bridges:?}");
    let init = hcl_nested(controller, "initialization");
    let user_data = init.first().map(|i| hcl_text(i.body(), "user_data_file_id"));
    assert_eq!(user_data.as_deref(), Some("proxmox_virtual_environment_file.isoloom_controller.id"));
    // The inventory names each machine as the isoloom user with the controller's key.
    assert!(tf.contains("ansible_user=isoloom"));
    assert!(tf.contains("ansible_ssh_private_key_file=/etc/isoloom/id_ed25519"));
    // It waits for each machine's ready file to say `ready` (and stops on `failed:`), then runs
    // the playbooks.
    assert!(tf.contains("head -n 1 /var/lib/isoloom/ready"));
    assert!(tf.contains("in \\\"failed: \\\"*)"), "{tf}");
    assert!(tf.contains("ansible-playbook -i /etc/isoloom/inventory.ini"));
    // It starts after every machine exists.
    let deps: Vec<String> = match hcl_attribute(controller, "depends_on") {
        Some(hcl::Expression::Array(items)) => items.iter().map(ToString::to_string).collect(),
        _ => Vec::new(),
    };
    for vm in ["proxmox_virtual_environment_vm.isoloom_router", "proxmox_virtual_environment_vm.web"] {
        assert!(deps.contains(&vm.to_owned()), "{vm}: {deps:?}");
    }
    Ok(())
}

#[test]
fn proxmox_without_environment_provisioning_has_no_controller() -> Outcome {
    let spec = example("hello-stack")?;
    let tf = &generate(&spec, Target::Proxmox).unwrap()[0].contents;
    let body = hcl_body(tf);
    assert!(!hcl_names(&body, "resource", "proxmox_virtual_environment_vm").contains(&"isoloom_controller".to_owned()));
    assert_eq!(hcl_names(&body, "resource", "tls_private_key"), Vec::<String>::new());
    // Without a controller, the isoloom user exists only when the operator gives a key.
    assert!(tf.contains("= var.ssh_public_key == \"\" ? [] : [{"), "{tf}");
    Ok(())
}

#[test]
fn an_uplink_check_reads_no_address_when_the_provider_reports_none_yet() -> Outcome {
    // Regression: a router whose guest agent hadn't answered left `ipv4_addresses` null, and the
    // postcondition's `flatten(null)` failed the apply with Terraform's "Invalid function
    // argument" instead of its own message ("got no IPv4 address on its uplink bridge").
    let spec = example("solo-web")?;
    let files = generate(&spec, Target::Proxmox).expect("solo-web runs on Proxmox");
    let tf = &files[0].contents;
    assert!(tf.contains("try(flatten(self.ipv4_addresses), [])"), "{tf}");
    assert!(!tf.contains("in flatten(self.ipv4_addresses) :"), "a bare flatten left: {tf}");
    Ok(())
}
