//! What a VM target can't host yet, read from the plan without rendering anything: a target
//! (Proxmox, ESXi, each cloud) declares its limits, each with the reason it gives, and adds the
//! checks only it has (its images, its address rules) after them.

use isoloom_plan::Environment;

/// The features a VM target lacks, each with its reason (`None`: the target has it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Limits {
    /// Machines on arm64 (the reason follows "machine `<name>`: ").
    pub(crate) arm64: Option<&'static str>,
    /// Networks routed by one of the environment's machines (`gateway:`).
    pub(crate) gateways: Option<&'static str>,
    /// Checks that are Ansible playbooks.
    pub(crate) playbook_checks: Option<&'static str>,
    /// Link impairment (`tc`).
    pub(crate) tc: Option<&'static str>,
    /// IPv6 networks (`ipv6`): why the cloud's network can't carry the spec's block.
    pub(crate) ipv6: Option<&'static str>,
}

impl Limits {
    /// The first of the limits the environment runs into, in this order: arm64, gateways,
    /// playbook checks, link impairment, IPv6.
    pub(crate) fn refusal(&self, env: &Environment) -> Option<String> {
        let spec = env.spec();
        if let (Some(why), Some(name)) = (self.arm64, isoloom_render::arm64_machine(spec)) {
            return Some(format!("machine `{name}`: {why}"));
        }
        if let Some(why) = self.gateways.filter(|_| env.networks.values().any(|n| n.gateway.is_some())) {
            return Some(why.to_owned());
        }
        if let Some(why) = self.playbook_checks.filter(|_| spec.checks.iter().any(isoloom_spec::Check::is_playbook)) {
            return Some(why.to_owned());
        }
        if let Some(why) = self.tc.filter(|_| env.networks.values().any(|n| n.tc.is_some())) {
            return Some(why.to_owned());
        }
        let v6 = env.networks.iter().find(|(_, n)| n.ipv6.is_some());
        self.ipv6.zip(v6).map(|(why, (name, _))| format!("network `{name}` is IPv6: {why}"))
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::testing::environment;

    #[test]
    fn the_first_limit_the_environment_runs_into_refuses_it() {
        let env = environment(
            "version: 1\nname: l\nnetworks:\n  lab: { cidr: 10.9.0.0/24, tc: { delay: 10ms } }\nmachines:\n  a: { networks: { lab: 10 }, os: debian@12, app: { steps: [{ run: a.sh }] } }\n",
            Target::CloudVm,
        );
        let limits = Limits {
            arm64: Some("no arm64"),
            gateways: Some("no gateways"),
            playbook_checks: None,
            tc: Some("no tc"),
            ipv6: None,
        };
        assert_eq!(limits.refusal(&env).as_deref(), Some("no tc"));
        assert_eq!(Limits::default().refusal(&env), None);
    }
}
