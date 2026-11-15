//! Environment-level `provision:` on Proxmox: a controller VM runs the playbooks, as on Vagrant
//! and the clouds. Before this, Proxmox refused such environments outright.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{hcl, hcl_attribute, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_text};

/// The module's `locals` attribute `key`, as HCL writes it.
fn local(body: &hcl::Body, key: &str) -> Outcome<String> {
    let found: Vec<String> = hcl_nested(body, "locals")
        .iter()
        .filter(|b| hcl_attribute(b.body(), key).is_some())
        .map(|b| hcl_text(b.body(), key))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(format!("{} `local.{key}`", found.len()).into()),
    }
}

/// Whether one literal of `values` holds `text` (a line of a set-up script).
fn holds(values: &[String], text: &str) -> bool {
    values.iter().any(|v| v.contains(text))
}

use crate::testing::{Outcome, example};

/// The controller's set-up: an inventory naming each machine as the isoloom user with the
/// controller's key, a wait for each machine's ready file to say `ready` (stopping on `failed:`),
/// then the playbooks.
fn the_controller_sets_up_the_inventory_waits_and_runs_the_playbooks(setup: &[String]) -> Outcome {
    let inventory = setup
        .iter()
        .find(|v| v.starts_with("[linux]\n"))
        .ok_or("no inventory in the controller's set-up")?;
    for machine in ["web", "cache"] {
        let line = inventory.lines().find(|l| l.starts_with(&format!("{machine} "))).unwrap_or_default();
        assert!(line.split(' ').any(|w| w == "ansible_user=isoloom"), "{machine}: {inventory}");
    }
    let vars = inventory.split("[linux:vars]\n").nth(1).unwrap_or_default();
    assert!(vars.lines().any(|l| l == "ansible_ssh_private_key_file=/etc/isoloom/id_ed25519"), "{inventory}");
    let waits: Vec<&String> = setup.iter().filter(|v| v.starts_with("echo 'running: waiting for ")).collect();
    assert_eq!(waits.len(), 2, "a wait for each machine: {setup:?}");
    for wait in waits {
        assert!(
            wait.contains("head -n 1 /var/lib/isoloom/ready") && wait.contains("in \"failed: \"*)"),
            "{wait}"
        );
    }
    assert!(holds(setup, "ansible-playbook -i /etc/isoloom/inventory.ini"), "{setup:?}");
    Ok(())
}

#[test]
fn proxmox_runs_environment_playbooks_from_a_controller() -> Outcome {
    let spec = example("ansible-pair")?;
    let files = generate(&spec, Target::Proxmox).expect("environment provisioning is no longer refused on Proxmox");
    let tf = &files[0].contents;
    let body = hcl_body(tf);
    // Its own key, given to the `isoloom` user on every machine alongside the operator's.
    assert_eq!(hcl_names(&body, "resource", "tls_private_key"), ["controller"]);
    let users = local(&body, "users")?;
    assert!(
        users.contains("ssh_authorized_keys = compact([var.ssh_public_key, trimspace(tls_private_key.controller.public_key_openssh)])"),
        "the key handed on: {users}"
    );
    // The controller VM: on the uplink (its route out, to install Ansible) and on every network.
    let controller = hcl_one(&body, "proxmox_virtual_environment_vm", "isoloom_controller");
    let bridges: Vec<String> = hcl_nested(controller, "network_device").iter().map(|d| hcl_text(d.body(), "bridge")).collect();
    assert_eq!(bridges.first().map(String::as_str), Some("var.uplink_bridge"), "{bridges:?}");
    assert!(bridges.len() > 1, "and a card on every network: {bridges:?}");
    let init = hcl_nested(controller, "initialization");
    let user_data = init.first().map(|i| hcl_text(i.body(), "user_data_file_id"));
    assert_eq!(user_data.as_deref(), Some("proxmox_virtual_environment_file.isoloom_controller.id"));
    the_controller_sets_up_the_inventory_waits_and_runs_the_playbooks(&hcl_literals(hcl_one(&body, "proxmox_virtual_environment_file", "isoloom_controller")))?;
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
    let users = match hcl_nested(&body, "locals").iter().find_map(|b| hcl_attribute(b.body(), "users")) {
        Some(hcl::Expression::Conditional(c)) => (c.cond_expr.to_string(), c.true_expr.to_string()),
        other => return Err(format!("`local.users` isn't a conditional: {other:?}").into()),
    };
    assert_eq!(users, ("var.ssh_public_key == \"\"".to_owned(), "[]".to_owned()), "{tf}");
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
    let body = hcl_body(tf);
    let router = hcl_one(&body, "proxmox_virtual_environment_vm", "isoloom_router");
    let conditions: Vec<String> = hcl_nested(router, "lifecycle")
        .iter()
        .flat_map(|l| hcl_nested(l.body(), "postcondition"))
        .map(|p| hcl_text(p.body(), "condition"))
        .collect();
    let [condition] = conditions.as_slice() else {
        return Err(format!("{} postconditions: {tf}", conditions.len()).into());
    };
    let router_address = local(&body, "router_address")?;
    for read in [condition, &router_address] {
        assert!(
            read.contains("for a in try(flatten(self.ipv4_addresses), []) :")
                || read.contains("for a in try(flatten(proxmox_virtual_environment_vm.isoloom_router.ipv4_addresses), []) :"),
            "{read}"
        );
        assert!(!read.contains("in flatten("), "a bare flatten left: {read}");
    }
    Ok(())
}
