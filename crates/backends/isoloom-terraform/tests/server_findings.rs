//! What the real smoke tests on a Proxmox VE 9.2 server and an ESXi 8.0U3e host found, each
//! pinned by the module text that fixes it: the SDN deletions applied on destroy, no default node
//! name, a router that can't report its uplink address failing early and saying why, and the
//! bootstrap's progress read from each machine's ready file.

pub mod testing;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose};
use isoloom_spec::model::Target;
use isoloom_terraform::{EsxiBackend, ProxmoxBackend};
use testing::{Outcome, example, file, planned};

fn proxmox(name: &str) -> Outcome<String> {
    let env: Environment = planned(&example(name)?, Target::Proxmox);
    Ok(file(&ProxmoxBackend.render(&env)?, "proxmox/main.tf")?.to_owned())
}

/// An empty guest, for a docker-vm module test that reads only the module (not what runs in the VM).
///
/// # Errors
///
/// When the status step's id isn't one (it is fixed, so this never fails).
fn no_guest() -> Outcome<GuestCompose> {
    Ok(GuestCompose {
        machines: Vec::new(),
        networks: Vec::new(),
        slots: Vec::new(),
        params: Vec::new(),
        start: Vec::new(),
        ready: Vec::new(),
        check_setup: Vec::new(),
        runners: Vec::new(),
        status: GuestCommand {
            id: isoloom_manifest::StepId::parse("ps")?,
            note: None,
            command: isoloom_manifest::Template::text("true"),
            timeout_s: 60,
        },
    })
}

/// The docker-vm Proxmox module's `main.tf` for `env`.
///
/// # Errors
///
/// When the module can't be rendered, or has no `main.tf`.
fn docker_vm_main_tf(env: &Environment) -> Outcome<String> {
    let files = isoloom_terraform::docker_vm_module(env, &no_guest()?)?;
    Ok(files.into_iter().find(|f| f.path.ends_with("main.tf")).ok_or("no main.tf")?.contents)
}

fn esxi(name: &str) -> Outcome<String> {
    let env: Environment = planned(&example(name)?, Target::Esxi);
    Ok(file(&EsxiBackend.render(&env)?, "esxi/main.tf")?.to_owned())
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
    let finalizer = block(&tf, "resource \"proxmox_sdn_applier\" \"finalizer\"");
    assert!(finalizer.starts_with("resource"), "a finalizer applier: {tf}");
    let zone = block(&tf, "resource \"proxmox_sdn_zone_simple\" \"env\"");
    assert!(zone.contains("depends_on = [proxmox_sdn_applier.finalizer]"), "{zone}");
    let applier = block(&tf, "resource \"proxmox_sdn_applier\" \"env\"");
    assert!(applier.contains("replace_triggered_by"), "{applier}");
    assert!(applier.contains("proxmox_sdn_zone_simple.env"), "{applier}");
    Ok(())
}

#[test]
fn the_node_has_no_default_name() -> Outcome {
    // Regression: `node` defaulted to `pve`, while a server's first node can be named anything
    // (the launcher's connection test passed on `pve-lab`, then the launch failed).
    let tf = proxmox("hello-stack")?;
    let node = block(&tf, "variable \"node\"");
    assert!(!node.contains("default"), "{node}");
    assert!(node.contains("validation"), "an empty node is refused: {node}");
    Ok(())
}

#[test]
fn a_router_that_cant_report_its_address_fails_within_minutes_and_says_why() -> Outcome {
    // Regression: with no DNS or internet on its uplink, the router's guest agent never installs,
    // and the apply waited the provider's 15 minutes, then failed on an empty list's index.
    let tf = proxmox("hello-stack")?;
    let router = block(&tf, "resource \"proxmox_virtual_environment_vm\" \"isoloom_router\"");
    assert!(router.contains("timeout = \"8m\""), "{router}");
    assert!(router.contains("wait_for_ip"), "{router}");
    assert!(router.contains("stop_on_destroy = true"), "a destroy never waits on a missing agent: {router}");
    assert!(router.contains("postcondition"), "{router}");
    assert!(router.contains("no DHCP"), "{router}");
    assert!(router.contains("qemu-guest-agent"), "{router}");
    Ok(())
}

#[test]
fn a_router_that_never_reported_an_address_can_still_be_destroyed() -> Outcome {
    // Found on the real server: after the postcondition failed, `destroy` stopped on "Invalid
    // index" (the router's address list is empty in the state), leaving the lab behind.
    let tf = proxmox("hello-stack")?;
    assert!(tf.contains("router_address = try(["), "{tf}");
    let lab =
        testing::spec("version: 1\nname: d\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n")?;
    let env = planned(&lab, Target::DockerVm);
    let host = docker_vm_main_tf(&env)?;
    assert!(host.contains("ip = try(["), "{host}");
    Ok(())
}

#[test]
fn an_esxi_router_uplink_without_dhcp_fails_instead_of_giving_its_lab_address() -> Outcome {
    // Regression: with no DHCP on the uplink port group, VMware Tools reported the router's lab
    // address (10.80.0.254 there), and the module output it as the lab's address.
    let tf = esxi("hello-stack")?;
    let router = block(&tf, "resource \"esxi_guest\" \"isoloom_router\"");
    assert!(router.contains("postcondition"), "{router}");
    assert!(router.contains("10.60.0.254"), "the lab address is told apart: {router}");
    assert!(router.contains("no DHCP"), "{router}");
    Ok(())
}

#[test]
fn the_lab_host_address_is_the_ip_output() -> Outcome {
    // The runner reads `ip` on every Terraform target; the server modules said `address`.
    for tf in [proxmox("hello-stack")?, esxi("hello-stack")?] {
        assert!(tf.contains("output \"ip\""), "{tf}");
        assert!(!tf.contains("output \"address\""), "{tf}");
        assert!(tf.contains("output \"ready_file\""), "{tf}");
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
    // Regression (finding 5): the ESXi `remote-exec` set-up wrote only `ready` (no
    // `running:`/`failed:` progress), unlike the other targets, so the runner had no progress to
    // report. Each step now marks the ready file first, and stops the set-up when it fails.
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
    // Regression (finding 4): a VM's /etc/hosts left its own names in the private zone out, so on
    // web `www.corp.internal` (its own alias) did not resolve, unlike the clouds' zones. A machine
    // now names itself and its aliases in the zone, as it does the others.
    let tf = proxmox("private-names")?;
    let web = block(&tf, "resource \"proxmox_virtual_environment_file\" \"web\"");
    assert!(web.contains("10.84.0.10 web www web.corp.internal www.corp.internal"), "{web}");
    let client = block(&tf, "resource \"proxmox_virtual_environment_file\" \"client\"");
    assert!(client.contains("10.84.0.20 client client.corp.internal"), "{client}");
    Ok(())
}
