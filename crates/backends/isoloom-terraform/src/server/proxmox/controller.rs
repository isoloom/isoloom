//! The controller on Proxmox: the core's controller as a cloud-init VM. It sits on the uplink
//! bridge (DHCP: internet, to install Ansible) and on every network at the controller address.
//! Its cloud-init carries the project, its key and the inventory; it then waits for each
//! machine's ready marker (over SSH, over WinRM on Windows) and runs the playbooks.

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Block, Module, list, reference, string};
use isoloom_render::steps;
use isoloom_spec::validate::Cidr;

use super::machine::{Boot, Vm, ip_config, nic, snippet};
use crate::machine::cloud_init::{self, CloudConfig, file};
use crate::machine::{Shell, controller, vms};
use crate::module::ident;

/// The controller's snippet and VM.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let mut config = CloudConfig::new("isoloom-controller", reference("local.users"));
    config.packages = ["python3-venv", "curl", "netcat-openbsd", "openssh-client"].map(String::from).to_vec();
    config.bootcmd.extend(cloud_init::prelude(env));
    config.file_lists.push(reference("local.project_files"));
    let inventory = controller::inventory_template(env, isoloom_render::inventory::Access::proxmox).into_expression();
    config.files = vec![
        file(
            "/etc/isoloom/id_ed25519",
            Some("0600"),
            reference("tls_private_key.controller.private_key_openssh"),
        ),
        file("/etc/isoloom/inventory.ini", Some("0644"), inventory),
    ];
    config.runcmd.extend(controller::hosts(env).map(|s| Shell::Root.command(&s)));
    config.runcmd.push(Shell::Root.command(controller::ANSIBLE));
    config.runcmd.extend(waits(env));
    config.runcmd.extend(controller::windows_waits(env).iter().map(|w| Shell::Root.command(w)));
    config.runcmd.push(Shell::Root.command(&controller::playbooks(env)));
    config.runcmd.extend(steps::linux(env, "", &Step::Ready).map(|s| Shell::Root.command(&s)));
    m.commented(
        "The controller: runs the environment's playbooks once every machine is up.",
        snippet("controller", "controller", config.document()),
    );
    // The uplink (DHCP, its route out) first, then every network at the controller address.
    let mut nics = vec![nic("var.uplink_bridge", None)];
    let mut ips = vec![ip_config("dhcp", None)];
    for (net, plan) in &env.networks {
        nics.push(super::links::device(env, net, None, None));
        let c = Cidr::parse(&plan.cidr);
        ips.push(ip_config(&c.map(|c| format!("{}/{}", c.controller(), c.len)).unwrap_or_default(), None));
    }
    let (cpus, memory, disk) = controller::size(env);
    let mut depends_on = vec![reference("proxmox_virtual_environment_vm.isoloom_router")];
    depends_on.extend(vms(env).map(|(n, _)| reference(&format!("proxmox_virtual_environment_vm.{}", ident(n)))));
    let vm = Vm {
        id: "isoloom_controller".to_owned(),
        snippet: "controller".to_owned(),
        name: "controller".to_owned(),
        size: (cpus, memory, disk.unwrap_or(8)),
        boot: Boot::Image(ident(controller::OS)),
        tpm: false,
        own_boot: None,
        arm: false,
        agent: false,
        nics,
        pci: Vec::new(),
        dns: Some(Block::builder("dns").add_attribute(("servers", list([string("1.1.1.1")]))).build()),
        ips,
        depends_on,
        data_disks: Vec::new(),
        folders: Vec::new(),
    };
    m.block(vm.block(env));
}

/// Each Linux machine's own set-up must be done (its ready marker) before the playbooks run.
fn waits(env: &Environment) -> Vec<String> {
    vms(env)
        .filter(|(_, mp)| !mp.is_windows())
        .filter_map(|(name, mp)| {
            let a = mp.addresses.values().next()?;
            Some(Shell::Root.command(
                &format!(
                    "i=0; until ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i /etc/isoloom/id_ed25519 isoloom@{a} test -f /var/lib/isoloom/ready 2>/dev/null; do i=$((i+1)); [ $i -gt 240 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 5; done"
                ),
            ))
        })
        .collect()
}
