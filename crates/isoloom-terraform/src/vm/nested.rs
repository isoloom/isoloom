//! Nested virtualization on a cloud (`nested_virtualization`): a machine running VMs of its own
//! gets a size whose CPUs expose their virtualization extensions, and the cloud's switch for
//! it (AWS `cpu_options.nested_virtualization`, Google Cloud
//! `advanced_machine_features.enable_nested_virtualization`, Oracle Cloud
//! `platform_config.are_virtual_instructions_enabled`). Clouds without one refuse it, with the
//! reason.

use isoloom_plan::{Environment, MachinePlan, MachineShape};

/// Whether the machine runs VMs of its own.
pub fn wanted(mp: &MachinePlan) -> bool {
    mp.hardware.nested_virtualization
}

/// Why `cloud` can't run the machines' VMs inside them, if it can't: it has no nesting at all
/// (`supported` false), or a machine asks for GPUs too (no GPU type here nests).
pub fn refusal(env: &Environment, cloud: &str, supported: bool) -> Option<String> {
    let (name, mp) = env.machines.iter().find(|(_, mp)| mp.shape == MachineShape::Vm && wanted(mp))?;
    if !supported {
        return Some(format!(
            "machine `{name}`: nested virtualization (`nested_virtualization`) on {cloud} comes later (AWS, Google Cloud and Oracle Cloud have it)"
        ));
    }
    mp.hardware
        .gpus
        .is_some()
        .then(|| format!("machine `{name}`: GPUs and nested virtualization together: no {cloud} instance type here has both"))
}

/// The size for a machine that nests, by its memory: the smallest of `sizes` (memory in MB,
/// size) with as much, else the largest. `None` when it doesn't nest.
pub fn size(mp: &MachinePlan, memory_mb: u32, sizes: &[(u32, &'static str)]) -> Option<&'static str> {
    if !wanted(mp) {
        return None;
    }
    sizes.iter().find(|(mb, _)| *mb >= memory_mb).or(sizes.last()).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nesting_size_has_enough_memory() {
        let sizes = [(8192, "small"), (16384, "large")];
        let env = |extra: &str| {
            let spec = isoloom_spec::parse(&format!(
                "version: 1\nname: n\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lab: 10 }}, vm: {{ os: debian-12, provision: [a.sh] }}, {extra} }}\n"
            ))
            .unwrap();
            Environment::build(&spec, isoloom_spec::model::Target::CloudVm, None)
        };
        let nests = env("nested_virtualization: true");
        let mp = &nests.machines["a"];
        assert_eq!(size(mp, 1024, &sizes), Some("small"));
        assert_eq!(size(mp, 32768, &sizes), Some("large"));
        assert_eq!(size(&env("arch: amd64").machines["a"], 1024, &sizes), None);
        assert!(refusal(&nests, "Acme", false).is_some_and(|w| w.contains("on Acme comes later")));
        assert_eq!(refusal(&nests, "Acme", true), None);
        let both = env("nested_virtualization: true, gpus: 1");
        assert!(refusal(&both, "Acme", true).is_some_and(|w| w.contains("GPUs and nested virtualization together")));
    }
}
