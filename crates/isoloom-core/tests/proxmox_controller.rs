//! Environment-level `provision:` on Proxmox: a controller VM runs the playbooks, as on Vagrant
//! and the clouds. Before this, Proxmox refused such labs outright.

use std::path::PathBuf;

use isoloom_core::{Target, generate, load};

fn example(name: &str) -> isoloom_core::Spec {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    load(&dir).expect("example parses")
}

#[test]
fn proxmox_runs_environment_playbooks_from_a_controller() {
    let spec = example("ansible-pair");
    let files = generate(&spec, Target::Proxmox).expect("environment provisioning is no longer refused on Proxmox");
    let tf = &files[0].contents;
    // Its own key, given to the `isoloom` user on every machine alongside the operator's.
    assert!(tf.contains("resource \"tls_private_key\" \"controller\""));
    assert!(tf.contains("trimspace(tls_private_key.controller.public_key_openssh)"));
    // The controller VM: on the uplink (its route out, to install Ansible) and on every network.
    assert!(tf.contains("resource \"proxmox_virtual_environment_vm\" \"isoloom_controller\""));
    assert!(tf.contains("bridge = var.uplink_bridge"));
    assert!(tf.contains("user_data_file_id = proxmox_virtual_environment_file.controller.id"));
    // The inventory names each machine as the isoloom user with the controller's key.
    assert!(tf.contains("ansible_user=isoloom"));
    assert!(tf.contains("ansible_ssh_private_key_file=/etc/isoloom/id_ed25519"));
    // It waits for each machine's own set-up to finish, then runs the playbooks.
    assert!(tf.contains("test -f /var/lib/isoloom/ready"));
    assert!(tf.contains("isoloom_play site.yml -i /etc/isoloom/inventory.ini"), "{tf}");
    // It starts after every machine exists.
    assert!(tf.contains("depends_on = [proxmox_virtual_environment_vm.isoloom_router, proxmox_virtual_environment_vm."));
}

#[test]
fn proxmox_without_environment_provisioning_has_no_controller() {
    let spec = example("hello-stack");
    let tf = &generate(&spec, Target::Proxmox).unwrap()[0].contents;
    assert!(!tf.contains("isoloom_controller"));
    assert!(!tf.contains("tls_private_key"));
    // Without a controller, the isoloom user exists only when the operator gives a key.
    assert!(tf.contains("users = var.ssh_public_key == \"\" ? [] : [{"));
}
