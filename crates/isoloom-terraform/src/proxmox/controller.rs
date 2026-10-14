//! The controller on Proxmox: the core's controller as a cloud-init VM. It sits on the uplink
//! bridge (DHCP: internet, to install Ansible) and on every network at the controller address.
//! Its cloud-init carries the project, its key and the inventory; it then waits for each
//! machine's ready marker (over SSH, over WinRM on Windows) and runs the playbooks.

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Module, reference, string};
use isoloom_render::steps;
use isoloom_spec::validate::Cidr;

use super::machine::{Boot, Vm, ip_config, nic, snippet};
use crate::vm::cloud_init::{self, CloudConfig, file};
use crate::vm::{as_root, controller, is_windows, res, vms};

/// The controller's snippet and VM.
pub fn blocks(env: &Environment, m: &mut Module) {
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
    config.runcmd.extend(controller::hosts(env).map(|s| as_root(&s, false)));
    config.runcmd.push(as_root(controller::ANSIBLE, false));
    config.runcmd.extend(waits(env));
    config.runcmd.extend(super::windows::waits(env).iter().map(|w| as_root(w, false)));
    config.runcmd.push(as_root(&controller::playbooks(env), false));
    config.runcmd.extend(steps::linux(env, "", &Step::Ready).map(|s| as_root(&s, false)));
    m.commented(
        "The controller: runs the environment's playbooks once every machine is up.",
        snippet("controller", "controller", config.document()),
    );
    // The uplink (DHCP, its route out) first, then every network at the controller address.
    let mut nics = vec![nic("var.uplink_bridge", None)];
    let mut ips = vec![ip_config("dhcp", None)];
    for (net, plan) in &env.networks {
        nics.push(nic(&format!("proxmox_sdn_vnet.{}.id", res(net)), None));
        let c = Cidr::parse(&plan.cidr);
        ips.push(ip_config(&c.map(|c| format!("{}/{}", c.controller(), c.len)).unwrap_or_default(), None));
    }
    let (cpus, memory, disk) = controller::size(env);
    let mut depends_on = vec![reference("proxmox_virtual_environment_vm.isoloom_router")];
    depends_on.extend(vms(env).map(|(n, _)| reference(&format!("proxmox_virtual_environment_vm.{}", res(n)))));
    let vm = Vm {
        id: "isoloom_controller".into(),
        snippet: "controller".into(),
        name: "controller".into(),
        size: (cpus, memory, disk.unwrap_or(8)),
        boot: Boot::Image(res(controller::OS)),
        tpm: false,
        agent: false,
        nics,
        dns: Some(
            isoloom_render::hcl_builder::Block::builder("dns")
                .add_attribute(("servers", isoloom_render::hcl_builder::list([string("1.1.1.1")])))
                .build(),
        ),
        ips,
        depends_on,
    };
    m.block(vm.block(env));
}

/// Each Linux machine's own set-up must be done (its ready marker) before the playbooks run.
fn waits(env: &Environment) -> Vec<String> {
    vms(env)
        .filter(|(_, mp)| !is_windows(mp))
        .filter_map(|(name, mp)| {
            let a = mp.addresses.values().next()?;
            Some(as_root(
                &format!(
                    "i=0; until ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i /etc/isoloom/id_ed25519 isoloom@{a} test -f /var/lib/isoloom/ready 2>/dev/null; do i=$((i+1)); [ $i -gt 240 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 5; done"
                ),
                false,
            ))
        })
        .collect()
}
