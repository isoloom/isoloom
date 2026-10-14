//! What DigitalOcean, Linode and Oracle Cloud share: they take single-network Linux labs, so
//! their refusals, capabilities, set-up hooks and published ports are the same, read from the
//! plan.

use isoloom_plan::{Capabilities, Environment, Feature, MachinePlan, Step};
use isoloom_render::backend::{self, Files, RenderError};
use isoloom_render::hcl_builder::Template;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::{Protocol, Target};

use crate::vm::cloud::{redirects, reschedule_auto_stop};
use crate::vm::limits::Limits;
use crate::vm::linux::Hooks;
use crate::vm::{controller, is_windows, os, vms};

/// What a single-network cloud takes, and how it says it doesn't.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// The cloud's name, as its refusals start (`DigitalOcean`).
    pub cloud: &'static str,
    /// Whether it takes one VM only (it can't pin private addresses).
    pub single_machine: bool,
    /// The features it lacks.
    pub limits: Limits,
    /// Whether it has an image for an OS.
    pub has_image: fn(&str) -> bool,
    /// The OSes it has images for, as its refusal lists them.
    pub images: &'static str,
}

/// The features every cloud lacks for now, `arm64` the cloud's own.
pub const fn limits(arm64: &'static str) -> Limits {
    Limits {
        arm64: Some(arm64),
        gateways: Some("networks with a `gateway` machine in the cloud come later"),
        playbook_checks: Some("Ansible checks (.yml) in the cloud come later"),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's firewalls have none"),
    }
}

/// What the plan may ask of these clouds; their own rules refuse the rest.
pub fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
    ])
}

/// Why the cloud can't run this environment, if it can't: one network, Linux only, no
/// controller, one VM when it says so, its limits, its images.
pub fn refusal(env: &Environment, rules: &Rules) -> Option<String> {
    let cloud = rules.cloud;
    if env.networks.len() != 1 {
        return Some(format!("{cloud} cloud-vm takes single-network labs so far"));
    }
    if vms(env).any(|(_, mp)| is_windows(mp)) {
        return Some(format!("{cloud} cloud-vm is Linux only so far"));
    }
    if controller::needed(env) {
        return Some(format!("{cloud} cloud-vm has no Ansible controller yet"));
    }
    if rules.single_machine && vms(env).count() > 1 {
        return Some(format!("{cloud} cloud-vm takes single-machine labs so far (it can't pin private addresses)"));
    }
    if let Some(why) = rules.limits.refusal(env) {
        return Some(why);
    }
    vms(env).find_map(|(name, mp)| {
        let os = os(mp).unwrap_or_default();
        (!(rules.has_image)(os)).then(|| format!("machine `{name}`: no {cloud} image for `{os}` yet ({})", rules.images))
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

/// The ports the VMs publish, as (protocol, port), TCP first, each once.
pub fn published_ports(env: &Environment) -> Vec<(Protocol, u16)> {
    let mut ports: Vec<(bool, u16, Protocol)> = vms(env)
        .flat_map(|(_, mp)| mp.ports.iter().map(|p| (!p.protocol.is_tcp(), p.publish, p.protocol)))
        .collect();
    ports.sort_unstable_by_key(|(udp, port, _)| (*udp, *port));
    ports.dedup_by_key(|(udp, port, _)| (*udp, *port));
    ports.into_iter().map(|(_, port, protocol)| (protocol, port)).collect()
}

/// The module's path: `.isoloom/cloud-vm/<sub>/main.tf`.
pub fn path(sub: &str) -> String {
    format!("{OUTPUT_DIR}/{}/{sub}/main.tf", crate::cloud_vm::DIR)
}
