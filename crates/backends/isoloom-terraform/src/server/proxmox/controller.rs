//! The controller on Proxmox: the core's controller as a cloud-init VM. It sits on the uplink
//! bridge (DHCP: internet, to install Ansible) and on every network at the controller address.
//! Its cloud-init carries the project, its key and the inventory; it trusts the site's
//! certificate authorities and installs Ansible, then waits for each machine's ready marker
//! (over SSH, over WinRM on Windows) and runs the playbooks.

use crate::hcl_builder::{Block, Module, list, reference, string};
use isoloom_plan::{Environment, Step};
use isoloom_render::inventory::{CONTROLLER_INVENTORY, CONTROLLER_KEY};
use isoloom_render::steps;
use isoloom_spec::net::Cidr;

use super::machine::{Boot, Vm, ip_config, nic, snippet};
use crate::machine::cloud_init::{self, CloudConfig, file};
use crate::machine::controller::OWNER_ONLY;
use crate::machine::{Shell, controller, vms};
use crate::module::ident;

/// The controller's snippet and VM.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    m.commented(
        "The controller: runs the environment's playbooks once every machine is up.",
        snippet(&ident(controller::NAME), controller::NAME, cloud_config(env).document()),
    );
    // The uplink (DHCP, its route out) first, then every network at the controller address.
    let mut nics = vec![nic("var.uplink_bridge", None)];
    let mut ips = vec![ip_config("dhcp", None)];
    for (net, plan) in &env.networks {
        nics.push(super::links::device(env, net, None, None));
        let c = Cidr::parse(&plan.cidr);
        ips.push(ip_config(&c.map(|c| format!("{}/{}", c.controller(), c.prefix())).unwrap_or_default(), None));
    }
    let (cpus, memory, disk) = controller::size(env);
    let mut depends_on = vec![reference("proxmox_virtual_environment_vm.isoloom_router")];
    depends_on.extend(vms(env).map(|(n, _)| reference(&format!("proxmox_virtual_environment_vm.{}", ident(n)))));
    let vm = Vm {
        id: "isoloom_controller".to_owned(),
        snippet: ident(controller::NAME),
        name: controller::NAME.to_owned(),
        size: (cpus, memory, disk),
        boot: Boot::Image(ident(controller::OS)),
        tpm: false,
        own_boot: None,
        arm: false,
        agent: None,
        nics,
        pci: Vec::new(),
        dns: Some(
            Block::builder("dns")
                .add_attribute(("servers", list(crate::server::resolvers(env, None).into_iter().map(string))))
                .build(),
        ),
        ips,
        depends_on,
        data_disks: Vec::new(),
        folders: Vec::new(),
    };
    m.block(vm.block(env));
}

/// Its cloud-init: the packages, the constraints, the project, its key and the inventory, then
/// the site's certificate authorities, the hosts file, Ansible, each machine's set-up awaited, the
/// playbooks and the health check, each tracked in its ready file.
fn cloud_config(env: &Environment) -> CloudConfig {
    let mut config = CloudConfig::new(controller::NAME, reference("local.users"));
    config.packages = controller::PACKAGES.map(String::from).to_vec();
    config.bootcmd.extend(cloud_init::prelude(env));
    config.file_lists.push(reference("local.project_files"));
    let inventory = controller::inventory_template(env, controller::server_access).into_expression();
    config.files = vec![
        file(CONTROLLER_KEY, Some(OWNER_ONLY), crate::machine::controller::private_key()),
        file(CONTROLLER_INVENTORY, Some(OWNER_ONLY), inventory),
    ];
    if let Some(trust) = steps::trust_authorities(env) {
        config.track("certificate authorities", &Shell::Root.command(&trust));
    }
    if let Some(hosts) = controller::hosts(env) {
        config.track("hosts file", &Shell::Root.command(&hosts));
    }
    config.track("installing Ansible", &Shell::Root.command(isoloom_render::controller::INSTALL_ANSIBLE));
    for (name, wait) in controller::linux_waits(env, &|_| crate::machine::USER.to_owned()) {
        config.track(&format!("waiting for {name}"), &wait);
    }
    for wait in controller::windows_waits(env) {
        config.track("waiting for the Windows machines", &Shell::Root.command(&wait));
    }
    config.track("the playbooks", &Shell::Root.command(&controller::playbooks(env)));
    if let Some(ready) = steps::linux(env, "", &Step::Ready) {
        config.track("health check", &Shell::Root.command(&ready));
    }
    config
}
