//! Google Cloud's `cloud-vm` module, `.isoloom/cloud-vm/gcp/main.tf`, rendered from the plan
//! through the VM core.
//!
//! - A project of its own (created and billed when none is given, deleted with the environment),
//!   a VPC network per network holding a subnetwork with its exact range, peered or routed
//!   through a machine (`network`); firewall rules matching each machine by its tag do what the
//!   AWS security groups do.
//! - Each machine is a `google_compute_instance` at its address with a public address, set up
//!   over SSH through the core's `remote-exec` transport. The SSH user comes from the
//!   `ssh-keys` metadata (created at first boot): the image's usual user, as on AWS. A machine
//!   on several networks has an interface in each VPC (up to 8, as many as its vCPUs, two at
//!   least), IP forwarding on so replies leave from the right one.
//! - A network a machine routes (`gateway:`): the machine takes the highest free address there,
//!   since Google Cloud keeps both the first address (the gateway's) and the second to last (the
//!   router's).
//!
//! What Google Cloud can't do the AWS way yet, so its refusal declines it: Windows, whose
//! first-boot set-up and WinRM differ enough that it comes later.

mod machines;
mod network;

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, NetworkPlan};
use isoloom_render::GeneratedFile;
use isoloom_render::hcl_builder::{Template, reference};
use isoloom_spec::images::is_windows;
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use super::{own_project, project, provider, public_ip, variables};
use crate::Result;
use crate::cloud::vm::gpus::GpuSizes;
use crate::cloud::vm::outputs::{self, Reach};
use crate::cloud::vm::rules::{Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes, kept_address};
use crate::cloud::vm::{self, gateways, own_image};
use crate::cloud::{Cloud, instance, operator};
use crate::limits::Limits;
use crate::machine::controller;
use crate::module::{self, ProjectCopy, Usage, ident, providers};

/// What Google Cloud takes.
const RULES: Rules = Rules {
    cloud: Cloud::Gcp,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("arm64 on Google Cloud comes later (Arm-based machine types and arm64 images)"),
        // A VPC per network: a route in one VPC to another's range goes through a machine.
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's firewall rules have none"),
        ipv6: Some("Google Cloud assigns a VPC's internal IPv6 range itself (inside fd20::/20), so the spec's block can't be kept"),
    },
    // NVIDIA L4s (G2, the GPUs attached). Projects get no quota for them until they ask, and
    // only some zones have them.
    gpus: GpuSizes {
        gpu: "NVIDIA L4",
        sizes: &[(1, "g2-standard-4"), (2, "g2-standard-24"), (4, "g2-standard-48"), (8, "g2-standard-96")],
        none: "",
    },
    nests: true,
    mtu: Mtu::Between { low: 1300, high: 8896 },
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 10, most: 15 },
    tpm: true,
    images: Images {
        label: "GCP",
        has: |os| image(os).is_some(),
        list: "Debian 12/13, Ubuntu 22.04/24.04",
        own: own_image::gcp,
    },
};

/// How the module writes its machines: Linux ones (its refusal declines Windows).
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: machines::linux,
    controller: machines::controller,
};

/// The machine types whose CPUs nest (N2: E2 has no nested virtualization), by memory in MB.
const NESTING: &[(u32, &str)] = &[(8192, "n2-standard-2"), (16384, "n2-standard-4"), (32768, "n2-standard-8")];

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
    image(os).map_or("admin", |(_, user)| user)
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

/// Why Google Cloud can't run this environment, if it can't: its features, its address rules,
/// then single-interface Linux machines with images, and the controller on one network.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    if let Some(why) = RULES.features(env) {
        return Some(why);
    }
    if env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).any(|c| c.len > 29) {
        return Some("Google Cloud subnetworks are /29 or larger".to_owned());
    }
    let machine = spec.machines.iter().find_map(|(name, m)| {
        let vm = m.vm.as_ref()?;
        if is_windows(&vm.os) {
            return Some(format!("machine `{name}`: Windows on GCP comes later"));
        }
        interfaces_refusal(env, name)
            .or_else(|| RULES.images.missing(name, &vm.os, own_image::of_machine(m, RULES.images.own)))
            .or_else(|| {
                kept_address(env, name, m, 1..=1)
                    .map(|net| format!("machine `{name}`: Google Cloud keeps a subnetwork's first address (.1, its gateway) on `{net}`"))
            })
    });
    if machine.is_some() {
        return machine;
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
fn relocate(env: &Environment, net: &str) -> Option<Ipv4Addr> {
    gateways::free_address(env, net, Cidr::parse(&env.networks.get(net)?.cidr)?)
}

/// A network's VPC's MTU: the one it sets, else Ethernet's 1500, as on every other target
/// (Google's default is 1460).
fn mtu(n: &NetworkPlan) -> u16 {
    n.mtu.unwrap_or(1500)
}

/// The module.
pub(crate) fn module(env: &Environment) -> Result<GeneratedFile> {
    let env = &*gateways::relocated_with(env, &|e, n, _| relocate(e, n));
    let folder = format!("{}/{}", Target::CloudVm.id(), Cloud::Gcp.id());
    let usage = Usage::with_key(&folder, "-var billing_account=<id> -var allowed_cidr=<your IP>/32 ");
    let mut m = module::start(&usage);
    m.block(providers::terraform(&vm::providers(env, &[providers::GOOGLE, providers::LOCAL], &[])));
    m.blocks(variables());
    m.blocks(operator::variables(env));
    m.block(provider());
    let project = project(env, &mut m);
    let zone = Template::new().with_expr(reference("var.region")).with_text("-a").into_expression();
    m.blocks(instance::id_and_locals(env, vec![("project", project), ("zone", zone)]));
    network::blocks(env, &mut m);
    vm::private_dns::gcp(env, own_project(), &mut m);
    vm::credentials(env, &mut m);
    MACHINES.write(env, &mut m);
    let reach = Reach {
        public_ip: &|n| public_ip(&ident(n)),
        user: &|mp| user(mp.os.as_deref().unwrap_or_default()).to_owned(),
        controller: Some((public_ip("isoloom_controller"), user(controller::OS))),
    };
    outputs::write(env, &reach, &mut m);
    module::file(Target::CloudVm, &folder, m, ProjectCopy::Archive)
}
