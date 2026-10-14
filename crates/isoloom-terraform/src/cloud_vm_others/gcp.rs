//! Google Cloud: `.isoloom/cloud-vm/gcp/main.tf`, rendered from the plan through the VM core.
//!
//! - A project of its own (created and billed when none is given, deleted with the environment),
//!   a VPC network per network holding a subnetwork with its exact range, peered or routed
//!   through a machine (`network`); firewall rules matching each machine by its tag do what the
//!   AWS security groups do.
//! - Each machine is a `google_compute_instance` at its address with a public address, set up
//!   over SSH through the core's `remote-exec` transport. The SSH user comes from the
//!   `ssh-keys` metadata (created at first boot): the image's usual user, as on AWS. A machine
//!   on several networks has an interface in each VPC (up to 8, as many as its vCPUs, two at least), IP
//!   forwarding on so replies leave from the right one.
//! - A network a machine routes (`gateway:`): the machine takes the highest free address there,
//!   since Google Cloud keeps both the first address (the gateway's) and the second to last (the
//!   router's).
//!
//! What Google Cloud can't do the AWS way yet, so its refusal declines it: Windows, whose
//! first-boot set-up and WinRM differ enough that it comes later.

mod machines;
mod network;

use isoloom_plan::{Capabilities, Environment, Step};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, attr, binary, boolean, conditional, index, null, number, reference, string,
};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, gcp_project_name, header};
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use crate::vm::cloud::{self, Reach, resource};
use crate::vm::gpus::{self, GpuSizes};
use crate::vm::limits::Limits;
use crate::vm::module::{Provider, Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{controller, gateways, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "gcp";

/// What Google Cloud can't host yet.
pub const LIMITS: Limits = Limits {
    arm64: Some("arm64 on Google Cloud comes later (Arm-based machine types and arm64 images)"),
    // A VPC per network: a route in one VPC to another's range goes through a machine.
    gateways: None,
    playbook_checks: Some("Ansible checks (.yml) in the cloud come later"),
    tc: Some("link impairment (`tc`) needs a router in the path; the cloud's firewall rules have none"),
    ipv6: Some("Google Cloud assigns a VPC's internal IPv6 range itself (inside fd20::/20), so the spec's block can't be kept"),
};

/// Google Cloud's GPU machine types: NVIDIA L4s (G2, the GPUs attached). Projects get no quota
/// for them until they ask, and only some zones have them.
pub const GPUS: GpuSizes = GpuSizes {
    cloud: "Google Cloud",
    gpu: "NVIDIA L4",
    sizes: &[(1, "g2-standard-4"), (2, "g2-standard-24"), (4, "g2-standard-48"), (8, "g2-standard-96")],
    none: "",
};

/// The machine types whose CPUs nest (N2: E2 has no nested virtualization), by memory in MB.
pub const NESTING: &[(u32, &str)] = &[(8192, "n2-standard-2"), (16384, "n2-standard-4"), (32768, "n2-standard-8")];

/// The `cloud-vm` module on Google Cloud.
#[derive(Debug, Clone, Copy, Default)]
pub struct GcpBackend;

impl Backend for GcpBackend {
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

/// The image family and SSH user for an OS. The user is created from the `ssh-keys` metadata,
/// so it is the AWS image's user and the inventory is the same.
fn image(os: &str) -> Option<(&'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("debian-cloud/debian-12", "admin"),
        "debian-13" => ("debian-cloud/debian-13", "admin"),
        "ubuntu-22.04" => ("ubuntu-os-cloud/ubuntu-2204-lts", "ubuntu"),
        "ubuntu-24.04" => ("ubuntu-os-cloud/ubuntu-2404-lts-amd64", "ubuntu"),
        _ => return None,
    })
}

/// The SSH user of an OS's image.
fn user(os: &str) -> &'static str {
    image(os).map_or("admin", |i| i.1)
}

/// The machine type for a memory size: E2 shared-core up to 4 GB, then standard.
fn machine_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "e2-small",
        1025..=2048 => "e2-medium",
        2049..=8192 => "e2-standard-2",
        _ => "e2-standard-4",
    }
}

/// Why Google Cloud can't run this environment, if it can't: its limits, its address rules,
/// then single-interface Linux machines with images, and the controller on one network.
pub fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    if let Some(why) = LIMITS
        .refusal(env)
        .or_else(|| gpus::refusal(env, &GPUS))
        .or_else(|| crate::vm::nested::refusal(env, "Google Cloud", true))
        .or_else(|| mtu_refusal(env))
        .or_else(|| crate::vm::disks::too_small(env, "Google Cloud", 10, 15))
    {
        return Some(why);
    }
    if env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).any(|c| c.len > 29) {
        return Some("Google Cloud subnetworks are /29 or larger".into());
    }
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if isoloom_spec::images::is_windows(&vm.os) {
            return Some(format!("machine `{name}`: Windows on GCP comes later"));
        }
        if let Some(why) = interfaces_refusal(env, name) {
            return Some(why);
        }
        if image(&vm.os).is_none() && crate::vm::own_image::of_machine(m, crate::vm::own_image::gcp).is_none() {
            return Some(format!("machine `{name}`: no GCP image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04)", vm.os));
        }
        // A gateway machine's gateway address moves to a free one (`vm::gateways`).
        if let Some((net, _)) = m.networks.iter().find(|(n, o)| **o == 1 && !gateways::is_gateway_address(env, name, n)) {
            return Some(format!(
                "machine `{name}`: Google Cloud keeps a subnetwork's first address (.1, its gateway) on `{net}`"
            ));
        }
    }
    if let Some((net, _)) = gateways::routed(env).into_iter().find(|(n, _)| relocate(env, n).is_none()) {
        return Some(format!(
            "network `{net}`: no address left for its gateway machine (Google Cloud keeps the router's)"
        ));
    }
    // The controller sits on every network, an interface on each.
    (controller::needed(env) && env.networks.len() > MAX_NICS).then(|| {
        format!(
            "the Ansible controller spans {} networks: a Google Cloud VM takes {MAX_NICS} interfaces at most",
            env.networks.len()
        )
    })
}

/// The most interfaces a Google Cloud VM takes (one per VPC network).
const MAX_NICS: usize = 8;

/// Why machine `name`'s networks can't each be an interface, if they can't: more than 8, or more
/// than its machine type's vCPUs allow (two at least).
fn interfaces_refusal(env: &Environment, name: &str) -> Option<String> {
    let mp = env.machines.get(name)?;
    let nics = mp.addresses.len();
    if nics > MAX_NICS {
        return Some(format!(
            "machine `{name}` is on {nics} networks: a Google Cloud VM takes {MAX_NICS} interfaces at most"
        ));
    }
    let t = machines::machine_type_of(mp);
    (vcpus(t) < nics).then(|| {
        format!(
            "machine `{name}` is on {nics} networks: a {t} takes {} interfaces at most (one per vCPU)",
            vcpus(t)
        )
    })
}

/// The vCPUs of a machine type (`e2-standard-4`: 4), as many interfaces as it takes; the shared
/// core types take two.
fn vcpus(t: &str) -> usize {
    t.rsplit('-').next().and_then(|n| n.parse().ok()).unwrap_or(2).max(2)
}

/// A machine type with room for `nics` interfaces: the E2 standard type with enough vCPUs when
/// the memory's own has too few (a GPU or nested type keeps its own; the refusal checks it).
fn for_nics(t: &'static str, nics: usize) -> &'static str {
    match t {
        _ if vcpus(t) >= nics || !t.starts_with("e2-") => t,
        _ if nics <= 4 => "e2-standard-4",
        _ => "e2-standard-8",
    }
}

/// Where a gateway machine goes on the network it routes: the highest address nothing else takes.
fn relocate(env: &Environment, net: &str) -> Option<std::net::Ipv4Addr> {
    gateways::free_address(env, net, Cidr::parse(&env.networks.get(net)?.cidr)?)
}

/// The MTUs a Google Cloud VPC takes.
const MTUS: std::ops::RangeInclusive<u16> = 1300..=8896;

/// A network's VPC's MTU: the one it sets, else Ethernet's 1500, as on every other target
/// (Google's default is 1460).
fn mtu(n: &isoloom_plan::NetworkPlan) -> u16 {
    n.mtu.unwrap_or(1500)
}

/// Why a network's MTU can't be its VPC's, if it can't.
fn mtu_refusal(env: &Environment) -> Option<String> {
    env.networks
        .iter()
        .find_map(|(name, n)| n.mtu.filter(|m| !MTUS.contains(m)).map(|m| (name, m)))
        .map(|(name, m)| format!("network `{name}`'s `mtu` ({m}): a Google Cloud VPC takes 1300 to 8896"))
}

/// The steps the VPC does itself: it routes between subnetworks.
fn vpc_does(step: &Step) -> bool {
    matches!(step, Step::Routes { .. } | Step::Trunks)
}

/// The module.
pub fn render(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let env = &*gateways::relocated_with(env, &|e, n, _| relocate(e, n));
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    m.block(terraform(&providers(env)));
    m.blocks(variables(env));
    m.block(
        Block::builder("provider")
            .add_label("google")
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    project(env, &mut m);
    network::blocks(env, &mut m);
    crate::vm::private_dns::gcp(env, own_project(), &mut m);
    if controller::needed(env) {
        m.commented("The controller's own key: it runs the playbooks over SSH on every machine.", controller::key());
    }
    for (name, _) in vms(env) {
        machines::linux(env, name, &mut m);
    }
    if controller::needed(env) {
        machines::controller(env, &mut m);
    }
    let reach = Reach {
        public_ip: &|n| public_ip(&res(n)),
        user: &|mp| user(crate::vm::os(mp).unwrap_or_default()).to_string(),
        controller: Some((public_ip("isoloom_controller"), user(controller::OS))),
    };
    cloud::outputs(env, &reach, &mut m);
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/{SUB}/main.tf", crate::cloud_vm::DIR),
        contents: m.render()?,
    })
}

const USAGE: &str = "# Start:  terraform -chdir=.isoloom/cloud-vm/gcp init && terraform -chdir=.isoloom/cloud-vm/gcp apply \\\n#           -var billing_account=<id> -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/gcp destroy (same variables)\n";

/// The address Terraform reaches an instance at (`id`: its resource name).
fn public_ip(id: &str) -> Expression {
    let nic = index(reference(&format!("google_compute_instance.{id}.network_interface")), 0);
    attr(index(attr(nic, "access_config"), 0), "nat_ip")
}

/// Google and local (the archive's file list); TLS for the controller's key.
fn providers(env: &Environment) -> Vec<Provider> {
    let mut p = vec![versions::GOOGLE, versions::LOCAL];
    if controller::needed(env) {
        p.push(versions::TLS);
    }
    p
}

fn variables(env: &Environment) -> Vec<Block> {
    let mut v = vec![
        Variable::new("project", "string")
            .default(string(""))
            .describe("An existing project; or leave empty and give billing_account for a project of its own")
            .block(),
        Variable::new("billing_account", "string")
            .default(string(""))
            .describe("With no project: create one for this environment, billed here, deleted with it")
            .block(),
        Variable::new("org_id", "string").default(string("")).block(),
        Variable::new("region", "string").default(string("europe-west9")).block(),
    ];
    v.extend(cloud::variables(env));
    v
}

/// `var.project == "" ? 1 : 0`: a project of its own when none is given.
fn own_project() -> Expression {
    conditional(binary(reference("var.project"), BinaryOperator::Eq, string("")), number(1), number(0))
}

/// The project (its own when none is given: everything goes with the environment), the
/// Compute API in it, the environment's random id, the names, the project and the zone.
fn project(env: &Environment, m: &mut Module) {
    let org = conditional(binary(reference("var.org_id"), BinaryOperator::Eq, string("")), null(), reference("var.org_id"));
    let id = Template::text("isoloom-").with_expr(reference("terraform_data.id.output"));
    m.commented(
        "A project of its own when none is given: everything goes when the environment is destroyed.",
        resource("google_project", "env")
            .add_attribute(("count", own_project()))
            .add_attribute(("name", string(gcp_project_name(&env.name))))
            .add_attribute(("project_id", id.into_expression()))
            .add_attribute(("billing_account", reference("var.billing_account")))
            .add_attribute(("org_id", org))
            .add_attribute(("deletion_policy", string("DELETE")))
            .add_attribute(("auto_create_network", boolean(false)))
            .build(),
    );
    m.block(
        resource("google_project_service", "compute")
            .add_attribute(("count", own_project()))
            .add_attribute(("project", attr(index(reference("google_project.env"), 0), "project_id")))
            .add_attribute(("service", string("compute.googleapis.com")))
            .add_attribute(("disable_on_destroy", boolean(false)))
            .build(),
    );
    let project = conditional(
        binary(reference("var.project"), BinaryOperator::NotEq, string("")),
        reference("var.project"),
        attr(index(reference("google_project_service.compute"), 0), "project"),
    );
    let zone = Template::new().with_expr(reference("var.region")).with_text("-a").into_expression();
    m.blocks(cloud::id_and_locals(env, vec![("project", project), ("zone", zone)]));
}
