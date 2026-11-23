//! Nested virtualization on a cloud (`nested_virtualization`): a machine running VMs of its own
//! gets a size whose CPUs expose their virtualization extensions, and the cloud's switch for
//! it (AWS `cpu_options.nested_virtualization`, Google Cloud
//! `advanced_machine_features.enable_nested_virtualization`, Oracle Cloud
//! `platform_config.are_virtual_instructions_enabled`). Clouds without one refuse it, with the
//! reason.

use isoloom_plan::{Environment, MachinePlan, MachineShape};

use crate::cloud::Cloud;

/// Whether the machine runs VMs of its own.
pub(crate) fn wanted(mp: &MachinePlan) -> bool {
    mp.hardware.nested_virtualization
}

/// Why `cloud` can't run the machines' VMs inside them, if it can't: it has no nesting at all
/// (`supported` false), or a machine asks for GPUs too (no GPU type here nests).
pub(crate) fn refusal(env: &Environment, cloud: Cloud, supported: bool) -> Option<String> {
    let (name, mp) = env.machines.iter().find(|(_, mp)| mp.shape == MachineShape::Vm && wanted(mp))?;
    if !supported {
        return Some(format!("machine `{name}`: {cloud} gives no nested virtualization (`nested_virtualization`)"));
    }
    mp.hardware
        .gpus
        .is_some()
        .then(|| format!("machine `{name}`: GPUs and nested virtualization together: no {cloud} instance type here has both"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_is_refused_where_the_cloud_has_none_or_with_gpus() {
        let env = |extra: &str| {
            crate::testing::planned(
                &format!(
                    "version: 1\nname: n\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lab: 10 }}, os: debian@12, app: {{ steps: [{{ run: a.sh }}] }}, hardware: {{ {extra} }} }}\n"
                ),
                isoloom_spec::model::Target::CloudVm,
            )
        };
        let nests = env("nested_virtualization: true");
        assert_eq!(refusal(&env("arch: amd64"), Cloud::Linode, false), None);
        assert!(refusal(&nests, Cloud::Linode, false).is_some_and(|w| w.contains("Linode gives no nested virtualization")));
        assert_eq!(refusal(&nests, Cloud::Linode, true), None);
        let both = env("nested_virtualization: true, gpus: 1");
        assert!(refusal(&both, Cloud::Linode, true).is_some_and(|w| w.contains("GPUs and nested virtualization together")));
    }
}
