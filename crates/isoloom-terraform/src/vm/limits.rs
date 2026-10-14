//! What a VM target can't host yet, read from the plan without rendering anything: a target
//! declares its limits, each with the reason it gives, and adds the checks only it has (its
//! images, its address rules) after them.

use isoloom_plan::Environment;

/// The features a VM target lacks, each with its reason (`None`: the target has it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Machines on arm64 (the reason follows "machine `<name>`: ").
    pub arm64: Option<&'static str>,
    /// Networks routed by one of the environment's machines (`gateway:`).
    pub gateways: Option<&'static str>,
    /// Checks that are Ansible playbooks.
    pub playbook_checks: Option<&'static str>,
    /// Link impairment (`tc`).
    pub tc: Option<&'static str>,
    /// IPv6 networks (`ipv6`): why the cloud's network can't carry the spec's block.
    pub ipv6: Option<&'static str>,
}

impl Limits {
    /// The first of the limits the environment runs into, in this order: arm64, gateways,
    /// playbook checks, link impairment, IPv6.
    pub fn refusal(&self, env: &Environment) -> Option<String> {
        let spec = env.spec();
        if let (Some(why), Some(name)) = (self.arm64, isoloom_render::arm64_machine(spec)) {
            return Some(format!("machine `{name}`: {why}"));
        }
        if let Some(why) = self.gateways.filter(|_| env.networks.values().any(|n| n.gateway.is_some())) {
            return Some(why.to_string());
        }
        if let Some(why) = self.playbook_checks.filter(|_| spec.checks.iter().any(isoloom_spec::Check::is_playbook)) {
            return Some(why.to_string());
        }
        if let Some(why) = self.tc.filter(|_| env.networks.values().any(|n| n.tc.is_some())) {
            return Some(why.to_string());
        }
        let v6 = env.networks.iter().find(|(_, n)| n.ipv6.is_some());
        self.ipv6.zip(v6).map(|(why, (name, _))| format!("network `{name}` is IPv6: {why}"))
    }
}
