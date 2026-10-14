//! Azure: `.isoloom/cloud-vm/azure/main.tf`, rendered from the plan through the VM core, the AWS
//! model on the `azurerm` provider.
//!
//! - One resource group and virtual network holding every network as a subnet with its exact
//!   range. The virtual network routes between subnets (the machines' `Routes` and `Trunks`
//!   steps are left out); a network security group per machine does what the router does on
//!   other targets: a machine's networks reach it, `reach` rules open one network to another
//!   (on their ports), the rest is closed.
//! - Each machine is a VM at its address with a static public IP, set up over SSH as `isoloom`
//!   (Azure creates the administrator; it reserves `admin`) through the core's `remote-exec`
//!   transport. A machine on several networks gets an interface on each (forwarding on, so
//!   replies leave from the right one), brought up by MAC at every boot.
//! - Windows (Azure's Server images): the `isoloom` administrator with a generated password,
//!   its computer name and a WinRM listener set at creation, then the core's WinRM set-up
//!   (NTLM); the auto-stop is a scheduled task, as Azure runs no user data on Windows.
//! - Environment-level `provision:`: the core's controller, a Debian VM on every network.

mod machines;
mod network;

use isoloom_plan::{Capabilities, Environment, Step};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, Module, null, reference, string};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use crate::vm::cloud::{self, Reach};
use crate::vm::limits::Limits;
use crate::vm::module::{Provider, Variable, project_lists, terraform};
use crate::vm::{controller, is_windows, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "azure";

/// Azure creates the administrator with the VM: a name it never reserves, on every machine.
const USER: &str = "isoloom";

/// What Azure can't host yet.
pub const LIMITS: Limits = Limits {
    arm64: Some("arm64 on Azure comes later (Arm-based sizes and arm64 images)"),
    gateways: Some("networks with a `gateway` machine in the cloud come later"),
    playbook_checks: Some("Ansible checks (.yml) in the cloud come later"),
    tc: Some("link impairment (`tc`) needs a router in the path; the cloud's security groups have none"),
};

/// The `cloud-vm` module on Azure.
#[derive(Debug, Clone, Copy, Default)]
pub struct AzureBackend;

impl Backend for AzureBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        crate::cloud_vm::capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        crate::cloud_vm::render_cloud(env, refusal, render)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        crate::cloud_vm::refuse_cloud(env, refusal)
    }
}

/// The Azure Marketplace image for an OS: (publisher, offer, sku).
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("Debian", "debian-12", "12-gen2"),
        "debian-13" => ("Debian", "debian-13", "13-gen2"),
        "ubuntu-22.04" => ("Canonical", "0001-com-ubuntu-server-jammy", "22_04-lts-gen2"),
        "ubuntu-24.04" => ("Canonical", "ubuntu-24_04-lts", "server-gen1"),
        "windows-server-2016" => ("MicrosoftWindowsServer", "WindowsServer", "2016-datacenter-gensecond"),
        "windows-server-2019" => ("MicrosoftWindowsServer", "WindowsServer", "2019-datacenter-gensecond"),
        "windows-server-2022" => ("MicrosoftWindowsServer", "WindowsServer", "2022-datacenter-azure-edition"),
        "windows-server-2025" => ("MicrosoftWindowsServer", "WindowsServer", "2025-datacenter-azure-edition"),
        _ => return None,
    })
}

/// The VM size for a machine's memory: B-series burstable, which covers the lab sizes.
fn size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "Standard_B1s",
        1025..=2048 => "Standard_B1ms",
        2049..=4096 => "Standard_B2s",
        4097..=8192 => "Standard_B2ms",
        _ => "Standard_B4ms",
    }
}

/// Why Azure can't run this environment, if it can't: its limits, its address rules, its
/// images, then what no cloud does yet for a Windows machine.
pub fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    if let Some(why) = LIMITS.refusal(env) {
        return Some(why);
    }
    if env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).any(|c| c.len > 29) {
        return Some("Azure subnets are /29 or larger".into());
    }
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if image(&vm.os).is_none() {
            return Some(format!(
                "machine `{name}`: no Azure image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04, Windows Server 2016 to 2025)",
                vm.os
            ));
        }
        if let Some(why) = cloud::windows_refusal(name, m) {
            return Some(why);
        }
        if let Some((net, _)) = m.networks.iter().find(|(_, o)| **o <= 3) {
            return Some(format!("machine `{name}`: Azure keeps the first addresses of a subnet (.1 to .3) on `{net}`"));
        }
    }
    cloud::windows_udp_redirect(spec)
}

/// The steps the virtual network does itself: it routes between subnets, each VLAN its own.
fn vnet_does(step: &Step) -> bool {
    matches!(step, Step::Routes { .. } | Step::Trunks)
}

/// The module.
pub fn render(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    m.block(terraform(&providers(env)));
    m.blocks(variables(env));
    m.blocks(base(env));
    network::blocks(env, &mut m);
    if vms(env).any(|(_, mp)| is_windows(mp)) {
        m.commented(
            "The Windows machines' administrator password (user isoloom), for WinRM.",
            cloud::windows_password(),
        );
    }
    if controller::needed(env) {
        m.commented("The controller's own key: it runs the playbooks over SSH on every machine.", controller::key());
    }
    for (name, mp) in vms(env) {
        if is_windows(mp) {
            machines::windows(env, name, &mut m);
        } else {
            machines::linux(env, name, &mut m);
        }
    }
    if controller::needed(env) {
        machines::controller(env, &mut m);
    }
    let reach = Reach {
        public_ip: &|n| public_ip(&res(n)),
        user: &|_| USER.to_string(),
        controller: Some((public_ip("isoloom_controller"), USER)),
    };
    cloud::outputs(env, &reach, &mut m);
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/{SUB}/main.tf", crate::cloud_vm::DIR),
        contents: m.render()?,
    })
}

const USAGE: &str = "# Start:  terraform -chdir=.isoloom/cloud-vm/azure init && terraform -chdir=.isoloom/cloud-vm/azure apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/azure destroy (same variables)\n";

/// The address Terraform reaches a machine at (`id`: its resource name).
fn public_ip(id: &str) -> isoloom_render::hcl_builder::Expression {
    reference(&format!("azurerm_public_ip.{id}.ip_address"))
}

/// Azure; TLS for the controller's key; random for the Windows password.
fn providers(env: &Environment) -> Vec<Provider> {
    let mut p = vec![("azurerm", "hashicorp/azurerm", "~> 5.0")];
    if controller::needed(env) {
        p.push(("tls", "hashicorp/tls", "~> 4.0"));
    }
    if vms(env).any(|(_, mp)| is_windows(mp)) {
        p.push(("random", "hashicorp/random", "~> 3.0"));
    }
    p
}

fn variables(env: &Environment) -> Vec<Block> {
    let mut v = vec![
        Variable::new("subscription_id", "string")
            .default(null())
            .describe("Default: ARM_SUBSCRIPTION_ID from the environment")
            .block(),
        Variable::new("region", "string").default(string("swedencentral")).block(),
    ];
    v.extend(cloud::variables(env));
    v
}

/// The provider, the environment's random id, the names and the tags every resource carries.
fn base(env: &Environment) -> Vec<Block> {
    let mut blocks = vec![
        Block::builder("provider")
            .add_label("azurerm")
            .add_block(Block::builder("features").build())
            .add_attribute(("subscription_id", reference("var.subscription_id")))
            .build(),
    ];
    blocks.extend(cloud::id_and_locals(env, vec![("tags", cloud::tags(env))]));
    blocks
}
