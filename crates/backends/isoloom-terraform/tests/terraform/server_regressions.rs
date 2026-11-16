//! What runs on a real Proxmox VE 9.2 server and an ESXi 8.0U3e host once failed on, each pinned
//! by the module text that makes it work: the SDN deletions applied on destroy, no default node
//! name, a router that can't report its uplink address failing early and saying why, and the
//! bootstrap's progress read from each machine's ready file.

use crate::testing::{Outcome, example, file, planned};
use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{EsxiBackend, ProxmoxBackend};
use isoloom_test_support::{hcl_attribute, hcl_blocks, hcl_body, hcl_names, hcl_nested, hcl_one, hcl_text};

fn proxmox(name: &str) -> Outcome<String> {
    let env: Environment = planned(&example(name)?, Target::Proxmox)?;
    Ok(file(&ProxmoxBackend.render(&env)?, "proxmox/main.tf")?.to_owned())
}

fn esxi(name: &str) -> Outcome<String> {
    let env: Environment = planned(&example(name)?, Target::Esxi)?;
    Ok(file(&EsxiBackend.render(&env)?, "esxi/main.tf")?.to_owned())
}

/// The references attribute `key` of `body` lists (`depends_on`, `replace_triggered_by`), as HCL
/// writes them.
fn references(body: &hcl::Body, key: &str) -> Vec<String> {
    match hcl_attribute(body, key) {
        Some(hcl::Expression::Array(items)) => items.iter().map(ToString::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Attribute `key` of the postcondition in `resource`'s lifecycle (`condition`,
/// `error_message`), as HCL writes it.
fn postcondition(resource: &hcl::Body, key: &str) -> Outcome<String> {
    let lifecycle = hcl_nested(resource, "lifecycle");
    let post = lifecycle.first().map(|l| hcl_nested(l.body(), "postcondition")).unwrap_or_default();
    let post = post.first().ok_or("no postcondition")?;
    Ok(hcl_attribute(post.body(), key).ok_or_else(|| format!("no {key}"))?.to_string())
}

/// The block of `tf` that starts with `head`, up to the next top-level block (empty when there
/// is none).
fn block(tf: &str, head: &str) -> String {
    let Some((_, rest)) = tf.split_once(head) else { return String::new() };
    let body = rest.split_once("\n}\n").map_or(rest, |(b, _)| b);
    format!("{head}{body}\n}}\n")
}

#[test]
fn a_destroy_applies_the_sdn_deletions_once_the_zone_is_gone() -> Outcome {
    // Regression: `destroy` removed the SDN applier before the zone and VNets, so their deletion
    // stayed pending on the server (the zone and its bridge were still there).
    let tf = proxmox("segmented")?;
    let body = hcl_body(&tf);
    assert_eq!(hcl_names(&body, "resource", "proxmox_sdn_applier"), ["finalizer", "env"], "a finalizer applier");
    let zone = hcl_one(&body, "proxmox_sdn_zone_simple", "env");
    assert_eq!(references(zone, "depends_on"), ["proxmox_sdn_applier.finalizer"]);
    let applier = hcl_one(&body, "proxmox_sdn_applier", "env");
    let lifecycle = hcl_nested(applier, "lifecycle");
    let triggers = lifecycle.first().map(|l| references(l.body(), "replace_triggered_by")).unwrap_or_default();
    assert!(triggers.iter().any(|t| t.starts_with("proxmox_sdn_zone_simple.env")), "{triggers:?}");
    Ok(())
}

#[test]
fn the_node_has_no_default_name() -> Outcome {
    // Regression: `node` defaulted to `pve`, while a server's first node can be named anything
    // (a connection test passed on a test server, then the launch failed).
    let tf = proxmox("hello-stack")?;
    let body = hcl_body(&tf);
    let node = hcl_blocks(&body, "variable", &["node"]);
    let node = node.first().ok_or("no node variable")?.body();
    assert!(hcl_attribute(node, "default").is_none(), "no default");
    assert_eq!(hcl_nested(node, "validation").len(), 1, "an empty node is refused");
    Ok(())
}

#[test]
fn a_router_that_cant_report_its_address_fails_within_minutes_and_says_why() -> Outcome {
    // Regression: with no DNS or internet on its uplink, the router's guest agent never installs,
    // and the apply waited the provider's 15 minutes, then failed on an empty list's index.
    let tf = proxmox("hello-stack")?;
    let body = hcl_body(&tf);
    let router = hcl_one(&body, "proxmox_virtual_environment_vm", "isoloom_router");
    let agent = hcl_nested(router, "agent");
    let agent = agent.first().ok_or("no agent block")?.body();
    assert_eq!(hcl_text(agent, "timeout"), "8m");
    assert_eq!(hcl_nested(agent, "wait_for_ip").len(), 1);
    assert_eq!(hcl_text(router, "stop_on_destroy"), "true", "a destroy never waits on a missing agent");
    let why = postcondition(router, "error_message")?;
    assert!(why.contains("no DHCP") && why.contains("qemu-guest-agent"), "{why}");
    Ok(())
}

#[test]
fn a_router_that_never_reported_an_address_can_still_be_destroyed() -> Outcome {
    // Found on the real server: after the postcondition failed, `destroy` stopped on "Invalid
    // index" (the router's address list is empty in the state), leaving the lab behind.
    let tf = proxmox("hello-stack")?;
    let body = hcl_body(&tf);
    let locals = hcl_blocks(&body, "locals", &[]);
    let address = locals
        .iter()
        .find_map(|l| hcl_attribute(l.body(), "router_address"))
        .ok_or("no router_address")?;
    assert!(matches!(address, hcl::Expression::FuncCall(f) if f.name.name.as_str() == "try"), "{address}");
    Ok(())
}

#[test]
fn an_esxi_router_uplink_without_dhcp_fails_instead_of_giving_its_lab_address() -> Outcome {
    // Regression: with no DHCP on the uplink port group, VMware Tools reported the router's lab
    // address (10.80.0.254 there), and the module output it as the lab's address.
    let tf = esxi("hello-stack")?;
    let body = hcl_body(&tf);
    let router = hcl_one(&body, "esxi_guest", "isoloom_router");
    let why = postcondition(router, "error_message")?;
    assert!(why.contains("no DHCP"), "{why}");
    // The condition tells the lab address apart from the uplink's.
    let condition = postcondition(router, "condition")?;
    assert!(condition.contains("!contains([\"10.60.0.254\"], self.ip_address)"), "{condition}");
    Ok(())
}

#[test]
fn the_lab_host_address_is_the_ip_output() -> Outcome {
    // The runner reads `ip` on every Terraform target; the server modules said `address`.
    for tf in [proxmox("hello-stack")?, esxi("hello-stack")?] {
        let outputs: Vec<String> = hcl_body(&tf)
            .blocks()
            .filter(|b| b.identifier() == "output")
            .filter_map(|b| b.labels().first().map(|l| l.as_str().to_owned()))
            .collect();
        assert!(outputs.iter().any(|o| o == "ip") && outputs.iter().any(|o| o == "ready_file"), "{outputs:?}");
        assert!(!outputs.iter().any(|o| o == "address"), "{outputs:?}");
    }
    Ok(())
}

#[test]
fn each_set_up_step_says_it_runs_and_a_failure_stops_the_rest() -> Outcome {
    // The ready file's grammar (`isoloom-ready/1`): `running: <step>` before each step,
    // `failed: <step>` and no further step when one fails, `ready` at the end.
    let tf = proxmox("hello-stack")?;
    let web = block(&tf, "resource \"proxmox_virtual_environment_file\" \"web\"");
    assert!(web.contains("running: provision/web.sh"), "{web}");
    assert!(web.contains("failed: provision/web.sh"), "{web}");
    assert!(web.contains("exit 1"), "{web}");
    assert!(web.contains("echo ready > /var/lib/isoloom/ready"), "{web}");
    Ok(())
}

#[test]
fn an_esxi_machine_reports_its_set_up_progress_in_its_ready_file() -> Outcome {
    // The ESXi `remote-exec` set-up reports its progress as every target's does: each step marks
    // the ready file (`running:`) first, and a failed one marks it (`failed:`) and stops the
    // set-up, so the runner has the progress to report.
    let tf = esxi("hello-stack")?;
    let web = block(&tf, "resource \"terraform_data\" \"web\"");
    assert!(web.contains("running: provision/web.sh"), "{web}");
    assert!(web.contains("failed: provision/web.sh"), "{web}");
    assert!(web.contains("running: health check"), "the final ready step is marked too: {web}");
    assert!(web.contains("echo ready > /var/lib/isoloom/ready"), "{web}");
    Ok(())
}

#[test]
fn a_vms_hosts_file_names_itself_in_the_zone() -> Outcome {
    // A VM's /etc/hosts names the VM itself and its aliases in the private zone, as it names the
    // others, so on web `www.corp.internal` (its own alias) resolves, as in the clouds' zones.
    let tf = proxmox("private-names")?;
    let web = block(&tf, "resource \"proxmox_virtual_environment_file\" \"web\"");
    assert!(web.contains("10.84.0.10 web www web.corp.internal www.corp.internal"), "{web}");
    let client = block(&tf, "resource \"proxmox_virtual_environment_file\" \"client\"");
    assert!(client.contains("10.84.0.20 client client.corp.internal"), "{client}");
    Ok(())
}
