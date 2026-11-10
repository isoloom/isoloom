//! Environment-level `provision:` on Proxmox: a controller VM runs the playbooks, as on Vagrant
//! and the clouds. Before this, Proxmox refused such labs outright.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, example, from};

#[test]
fn proxmox_runs_environment_playbooks_from_a_controller() -> Outcome {
    let spec = example("ansible-pair")?;
    let files = generate(&spec, Target::Proxmox).expect("environment provisioning is no longer refused on Proxmox");
    let tf = &files[0].contents;
    // Its own key, given to the `isoloom` user on every machine alongside the operator's.
    assert!(tf.contains("resource \"tls_private_key\" \"controller\""));
    assert!(tf.contains("trimspace(tls_private_key.controller.public_key_openssh)"));
    // The controller VM: on the uplink (its route out, to install Ansible) and on every network.
    assert!(tf.contains("resource \"proxmox_virtual_environment_vm\" \"isoloom_controller\""));
    assert!(tf.contains("bridge = var.uplink_bridge"));
    assert!(tf.contains("user_data_file_id = proxmox_virtual_environment_file.isoloom_controller.id"));
    // The inventory names each machine as the isoloom user with the controller's key.
    assert!(tf.contains("ansible_user=isoloom"));
    assert!(tf.contains("ansible_ssh_private_key_file=/etc/isoloom/id_ed25519"));
    // It waits for each machine's ready file to say `ready` (and stops on `failed:`), then runs
    // the playbooks.
    assert!(tf.contains("head -n 1 /var/lib/isoloom/ready"));
    assert!(tf.contains("in \\\"failed: \\\"*)"), "{tf}");
    assert!(tf.contains("ansible-playbook -i /etc/isoloom/inventory.ini"));
    // It starts after every machine exists.
    let vm = from(tf, "resource \"proxmox_virtual_environment_vm\" \"isoloom_controller\"")?;
    let deps = from(vm, "depends_on = [")?;
    assert!(
        deps.contains("proxmox_virtual_environment_vm.isoloom_router") && deps.contains("proxmox_virtual_environment_vm.web"),
        "{deps}"
    );
    Ok(())
}

#[test]
fn proxmox_without_environment_provisioning_has_no_controller() -> Outcome {
    let spec = example("hello-stack")?;
    let tf = &generate(&spec, Target::Proxmox).unwrap()[0].contents;
    assert!(!tf.contains("isoloom_controller"));
    assert!(!tf.contains("tls_private_key"));
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
