//! What DigitalOcean, Linode and Oracle Cloud share: Linux labs on the networks each cloud can
//! model ([`Networks`]), so their refusals, capabilities, set-up hooks and published ports are
//! the same, read from the plan.

use isoloom_plan::{Capabilities, Environment, MachinePlan, Step};
use isoloom_render::backend::{self, Files, RenderError};
use isoloom_render::hcl_builder::Template;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::{Protocol, Target};

use crate::vm::cloud::{redirects, reschedule_auto_stop};
use crate::vm::gpus::{self, GpuSizes};
use crate::vm::limits::Limits;
use crate::vm::linux::Hooks;
use crate::vm::{controller, is_windows, os, own_image, vms};

/// The networks a cloud can model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Networks {
    /// One network and one VM (DigitalOcean: a droplet joins one VPC, at an address the cloud
    /// picks, so the lab's fixed addressing holds only alone).
    OneVm,
    /// Several networks, each machine on one (Linode: an instance has one VPC interface).
    OnePerMachine,
    /// Several networks, a machine on several (Oracle Cloud: a VNIC per network).
    Several,
}

/// What one of these clouds takes, and how it says it doesn't.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// The cloud's name, as its refusals start (`DigitalOcean`).
    pub cloud: &'static str,
    /// The networks it can model.
    pub networks: Networks,
    /// The features it lacks.
    pub limits: Limits,
    /// Whether it has an image for an OS.
    pub has_image: fn(&str) -> bool,
    /// The OSes it has images for, as its refusal lists them.
    pub images: &'static str,
    /// Whether its VMs can have a virtual TPM.
    pub tpm: bool,
    /// The field of `vm.image` naming a machine's own image there, which stands in for its OS's.
    pub own: own_image::Field,
    /// Its GPU instance types.
    pub gpus: GpuSizes,
    /// Whether its VMs can run VMs of their own (`nested_virtualization`).
    pub nests: bool,
    /// Its smallest volume in GB, and the most a VM takes: a machine's data disks must fit.
    pub disks: (u32, usize),
}

/// The features every cloud lacks for now, `arm64`, `ipv6` and `gateways` (`None`: it has them)
/// the cloud's own.
pub const fn limits(arm64: &'static str, ipv6: &'static str, gateways: Option<&'static str>) -> Limits {
    Limits {
        arm64: Some(arm64),
        gateways,
        playbook_checks: Some("Ansible checks (.yml) in the cloud come later"),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's firewalls have none"),
        ipv6: Some(ipv6),
    }
}

/// Why the cloud's networks can't model the environment's, if they can't.
fn networks_refusal(env: &Environment, rules: &Rules) -> Option<String> {
    let cloud = rules.cloud;
    match rules.networks {
        Networks::OneVm if env.networks.len() != 1 || vms(env).count() > 1 => Some(format!(
            "{cloud} cloud-vm takes one machine on one network: a droplet joins one VPC, at a private address {cloud} picks, so other machines couldn't reach it at its spec address"
        )),
        Networks::OnePerMachine => vms(env)
            .find(|(_, mp)| mp.addresses.len() > 1)
            .map(|(name, _)| format!("machine `{name}` is on several networks: a {cloud} instance has one VPC interface")),
        _ => None,
    }
}

/// What the plan may ask of these clouds (every cloud's: `cloud_vm::capabilities`); their own
/// rules refuse the rest.
pub fn capabilities() -> Capabilities {
    crate::cloud_vm::capabilities()
}

/// Why the cloud can't run this environment, if it can't: its networks, Linux only, no
/// controller, its limits, its images.
pub fn refusal(env: &Environment, rules: &Rules) -> Option<String> {
    let cloud = rules.cloud;
    if let Some(why) = networks_refusal(env, rules) {
        return Some(why);
    }
    if vms(env).any(|(_, mp)| is_windows(mp)) {
        return Some(format!("{cloud} cloud-vm is Linux only so far"));
    }
    if controller::needed(env) {
        return Some(format!("{cloud} cloud-vm has no Ansible controller yet"));
    }
    if let Some(why) = rules
        .limits
        .refusal(env)
        .or_else(|| gpus::refusal(env, &rules.gpus))
        .or_else(|| crate::vm::nested::refusal(env, rules.cloud, rules.nests))
        .or_else(|| crate::vm::limits::fixed_mtu(env, rules.cloud))
        .or_else(|| crate::vm::disks::too_small(env, rules.cloud, rules.disks.0, rules.disks.1))
    {
        return Some(why);
    }
    if let Some((name, _)) = vms(env)
        .find(|(_, mp)| mp.tpm == Some(isoloom_spec::windows::Tpm::Required))
        .filter(|_| !rules.tpm)
    {
        return Some(format!("machine `{name}` asks for a virtual TPM (`vm.tpm: true`): {cloud} gives none"));
    }
    vms(env).find_map(|(name, mp)| {
        let os = os(mp).unwrap_or_default();
        (!(rules.has_image)(os) && own_image::of(mp, rules.own).is_none())
            .then(|| format!("machine `{name}`: no {cloud} image for `{os}` yet ({})", rules.images))
    })
}

/// The module, unless the plan or the cloud's rules refuse the environment.
pub fn render(
    env: &Environment,
    rules: &Rules,
    module: fn(&Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error>,
) -> Result<Files, RenderError> {
    backend::check(Target::CloudVm, env, &capabilities())?;
    if let Some(what) = refusal(env, rules) {
        return Err(unsupported(what));
    }
    let file = module(env).map_err(|e| unsupported(format!("the {} module couldn't be written: {e}", rules.cloud)))?;
    Ok(vec![file])
}

fn unsupported(what: String) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported { target: Target::CloudVm, what })
}

/// The steps the cloud's network does itself: one network, nothing to route.
fn network_does(step: &Step) -> bool {
    matches!(step, Step::Routes { .. } | Step::Trunks)
}

/// A machine's set-up hooks: the auto-stop again after a restart, the published-port redirects
/// before it closes itself off, `at_boot` first.
pub fn hooks(mp: &MachinePlan, at_boot: Vec<Template>) -> Hooks {
    Hooks {
        at_boot,
        after_reboot: vec![reschedule_auto_stop()],
        before_egress: redirects(mp),
        skip: network_does,
    }
}

/// The ports one machine publishes, as (protocol, port), TCP first, each once: a firewall opens
/// them on that machine only, never on its neighbors.
pub fn published_ports(mp: &MachinePlan) -> Vec<(Protocol, u16)> {
    let mut ports: Vec<(bool, u16, Protocol)> = mp.ports.iter().map(|p| (!p.protocol.is_tcp(), p.publish, p.protocol)).collect();
    ports.sort_unstable_by_key(|(udp, port, _)| (*udp, *port));
    ports.dedup_by_key(|(udp, port, _)| (*udp, *port));
    ports.into_iter().map(|(_, port, protocol)| (protocol, port)).collect()
}

/// The module's path: `.isoloom/cloud-vm/<sub>/main.tf`.
pub fn path(sub: &str) -> String {
    format!("{OUTPUT_DIR}/{}/{sub}/main.tf", crate::cloud_vm::DIR)
}
