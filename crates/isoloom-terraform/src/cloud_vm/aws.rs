//! AWS: `.isoloom/cloud-vm/aws/main.tf`, rendered from the plan through the VM core.
//!
//! - One VPC holding every network as a subnet with its exact range, so every address of the
//!   spec is kept. The VPC routes between subnets (the machines' `Routes` and `Trunks` steps are
//!   left out); security groups do what the router does on other targets: a network's machines
//!   reach each other, `reach` rules open one network to another (on their ports), the rest is
//!   closed.
//! - Each machine is an instance at its address, set up over SSH from where Terraform runs (the
//!   core's `remote-exec` transport). A machine on several networks gets a network interface on
//!   each (source/destination check off, so replies leave from the right one), brought up by MAC
//!   at every boot, and an Elastic IP on the first.
//! - Published services: open to `allowed_cidr` on the instance's public address; a published
//!   port that differs from the service's own is redirected inside the machine.
//! - Windows (Amazon's Server images): an `isoloom` administrator with a generated password and
//!   WinRM at first boot, then the core's WinRM set-up.
//! - Environment-level `provision:`: the core's controller, a Debian instance on every network.

mod machines;
mod network;
mod outputs;

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Block, Expression, Module, index, list, null, reference, string};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::images::is_windows as is_windows_os;
use isoloom_spec::validate::Cidr;

use crate::vm::cloud;
use crate::vm::limits::Limits;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{controller, is_windows, os, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "aws";

/// What AWS can't host yet.
pub const LIMITS: Limits = Limits {
    arm64: Some("arm64 on AWS comes later (Graviton instances and arm64 AMIs)"),
    gateways: Some("networks with a `gateway` machine in the cloud come later"),
    playbook_checks: Some("Ansible checks (.yml) in the cloud come later"),
    tc: Some("link impairment (`tc`) needs a router in the path; the cloud's security groups have none"),
    ipv6: Some("an AWS VPC takes an Amazon-provided (or your own registered) IPv6 block, never one the spec picks"),
};

/// The AMI lookup and SSH user for an OS name: (owner, name filter, ssh user).
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("136693071363", "debian-12-amd64-*", "admin"),
        "debian-13" => ("136693071363", "debian-13-amd64-*", "admin"),
        "ubuntu-22.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-jammy-22.04-amd64-server-*", "ubuntu"),
        "ubuntu-24.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-noble-24.04-amd64-server-*", "ubuntu"),
        // Amazon's Windows Server images (license included, billed per hour).
        "windows-server-2016" => ("801119661308", "Windows_Server-2016-English-Full-Base-*", "isoloom"),
        "windows-server-2019" => ("801119661308", "Windows_Server-2019-English-Full-Base-*", "isoloom"),
        "windows-server-2022" => ("801119661308", "Windows_Server-2022-English-Full-Base-*", "isoloom"),
        "windows-server-2025" => ("801119661308", "Windows_Server-2025-English-Full-Base-*", "isoloom"),
        _ => return None,
    })
}

/// The SSH user of an OS's image (`admin` for an OS without one).
pub fn image_user(os: &str) -> &'static str {
    image(os).map_or("admin", |i| i.2)
}

/// The instance type for a machine's memory: Free Tier eligible up to 8 GB (accounts on the
/// AWS Free plan can't launch other types), then general purpose.
pub fn instance_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "t3.micro",
        1025..=2048 => "t3.small",
        2049..=4096 => "c7i-flex.large",
        4097..=8192 => "m7i-flex.large",
        _ => "m7i-flex.xlarge",
    }
}

/// Why AWS can't run this environment, if it can't: its limits, then its address rules and
/// images. This refuses the whole `cloud-vm` target (AWS is its reference cloud).
pub fn refusal(env: &Environment) -> Option<String> {
    LIMITS.refusal(env).or_else(|| networks_refusal(env)).or_else(|| machines_refusal(env))
}

fn networks_refusal(env: &Environment) -> Option<String> {
    let nets: Vec<Cidr> = env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).collect();
    if nets.iter().any(|c| c.len > 28) {
        return Some("AWS subnets are /28 or larger".into());
    }
    let family = |c: &Cidr| c.base >> 24;
    if nets.iter().any(|c| family(c) != nets.first().map_or(0, family)) {
        return Some("AWS can't mix private ranges (10/8, 172.16/12, 192.168/16) in one VPC".into());
    }
    windows_udp_redirect(env.spec())
}

pub use crate::vm::cloud::windows_udp_redirect;

fn machines_refusal(env: &Environment) -> Option<String> {
    for (name, m) in &env.spec().machines {
        let Some(vm) = &m.vm else { continue };
        if image(&vm.os).is_none() {
            return Some(format!(
                "machine `{name}`: no AWS image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04, Windows Server 2016 to 2025)",
                vm.os
            ));
        }
        if let Some(why) = cloud::windows_refusal(name, m) {
            return Some(why);
        }
        if is_windows_os(&vm.os) && vm.tpm == Some(true) {
            return Some(format!(
                "machine `{name}`: a TPM on a Windows VM on AWS needs Amazon's NitroTPM images (TPM-Windows_Server-*), which come later"
            ));
        }
        if let Some((net, _)) = m.networks.iter().find(|(_, o)| **o <= 3) {
            return Some(format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"));
        }
    }
    None
}

/// The steps the VPC does itself: it routes between subnets, each VLAN its own.
fn vpc_does(step: &Step) -> bool {
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
    m.blocks(amis(env));
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
    outputs::blocks(env, &mut m);
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/{SUB}/main.tf", super::DIR),
        contents: m.render()?,
    })
}

const USAGE: &str = "# Start:  terraform -chdir=.isoloom/cloud-vm/aws init && terraform -chdir=.isoloom/cloud-vm/aws apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/aws destroy (same variables)\n";

/// AWS and local (the archive's file list); TLS for the controller's key; random and time for
/// the Windows machines.
fn providers(env: &Environment) -> Vec<crate::vm::module::Provider> {
    let mut p = vec![versions::AWS, versions::LOCAL];
    if controller::needed(env) {
        p.push(versions::TLS);
    }
    if vms(env).any(|(_, mp)| is_windows(mp)) {
        p.extend([versions::RANDOM, versions::TIME]);
    }
    p
}

fn variables(env: &Environment) -> Vec<Block> {
    let mut v = vec![Variable::new("region", "string").default(string("eu-west-3")).block()];
    v.extend(cloud::variables(env));
    v
}

/// The provider (every resource tagged as the environment's), the environment's random id, the
/// names and the zones.
fn base(env: &Environment) -> Vec<Block> {
    let tags = cloud::tags(env);
    let mut blocks = vec![
        Block::builder("provider")
            .add_label("aws")
            .add_attribute(("region", reference("var.region")))
            .add_block(Block::builder("default_tags").add_attribute(("tags", tags)).build())
            .build(),
    ];
    blocks.extend(cloud::id_and_locals(env, Vec::new()));
    blocks.push(
        Block::builder("data")
            .add_label("aws_availability_zones")
            .add_label("available")
            .add_attribute(("state", string("available")))
            .build(),
    );
    blocks
}

/// An AMI lookup per OS (the controller's among them).
fn amis(env: &Environment) -> Vec<Block> {
    let mut oses: Vec<&str> = vms(env).filter_map(|(_, mp)| os(mp)).collect();
    if controller::needed(env) {
        oses.push(controller::OS);
    }
    oses.sort_unstable();
    oses.dedup();
    oses.into_iter()
        .filter_map(|o| image(o).map(|(owner, filter, _)| (o, owner, filter)))
        .map(|(o, owner, filter)| {
            let f = |name: &str, value: &str| {
                Block::builder("filter")
                    .add_attribute(("name", string(name)))
                    .add_attribute(("values", list([string(value)])))
                    .build()
            };
            Block::builder("data")
                .add_label("aws_ami")
                .add_label(res(o))
                .add_attribute(("most_recent", isoloom_render::hcl_builder::boolean(true)))
                .add_attribute(("owners", list([string(owner)])))
                .add_block(f("name", filter))
                .add_block(f("architecture", "x86_64"))
                .build()
        })
        .collect()
}

/// An instance's user data on Linux: the auto-stop, scheduled at first boot.
fn linux_user_data() -> Expression {
    cloud::auto_stop(cloud::shutdown_script(), null())
}

/// `<resource>.<name>[.<attr>]`, as a reference.
fn r(path: &str) -> Expression {
    reference(path)
}

/// The zone the subnets are in.
fn first_zone() -> Expression {
    index(r("data.aws_availability_zones.available.names"), 0)
}
