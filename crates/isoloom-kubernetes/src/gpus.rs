//! A machine's GPUs on Kubernetes: a count of the device plugin's extended resource
//! (`nvidia.com/gpu`, `amd.com/gpu`, `gpu.intel.com/i915`) in the container's limits, which every
//! cluster with GPUs offers, and NVIDIA's capabilities in the container's environment. The
//! scheduler picks the GPUs: a spec naming them (`ids`), asking for all of a node's or giving
//! the runtime options is refused, with the reason.

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::{GpuCount, Gpus};

use crate::objects::{Container, EnvVar, Limits, NameValue, Resources};

/// The environment variable NVIDIA's container toolkit reads the capabilities from.
const NVIDIA_CAPABILITIES: &str = "NVIDIA_DRIVER_CAPABILITIES";

/// The device plugin's resource for a driver, if Isoloom knows it.
fn resource(driver: &str) -> Option<&'static str> {
    Some(match driver {
        "nvidia" => "nvidia.com/gpu",
        "amd" => "amd.com/gpu",
        "intel" => "gpu.intel.com/i915",
        _ => return None,
    })
}

/// Why Kubernetes can't give a container its GPUs as the spec asks, if it can't.
pub fn refusal(env: &Environment) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .find_map(|(name, mp)| mp.hardware.gpus.as_ref().and_then(|g| why(name, g)))
}

fn why(name: &str, g: &Gpus) -> Option<String> {
    let at = format!("machine `{name}`");
    if g.count == Some(GpuCount::All) {
        return Some(format!(
            "{at}: `gpus: all` has no Kubernetes form (a pod asks for a count of GPUs): give a count"
        ));
    }
    if !g.ids.is_empty() {
        return Some(format!("{at}: `gpus.ids`: Kubernetes' scheduler picks the GPUs; give a `count` instead"));
    }
    if !g.options.is_empty() {
        return Some(format!(
            "{at}: `gpus.options` are Docker's GPU driver options, which Kubernetes has no place for"
        ));
    }
    if resource(g.driver()).is_none() {
        return Some(format!(
            "{at}: no Kubernetes device plugin resource known for the `{}` driver (nvidia, amd and intel are)",
            g.driver()
        ));
    }
    (!g.capabilities.is_empty() && g.driver() != "nvidia").then(|| {
        format!(
            "{at}: `gpus.capabilities` are NVIDIA's container toolkit's: leave them out for `{}` GPUs",
            g.driver()
        )
    })
}

/// Adds the GPUs (a count of the driver's resource, in the limits) and NVIDIA's capabilities (in
/// the environment) to the machine's container.
pub fn apply(gpus: Option<&Gpus>, c: &mut Container) {
    let Some(g) = gpus else { return };
    let (Some(name), Some(count)) = (resource(g.driver()), g.fixed_count()) else {
        return;
    };
    let resources = c.resources.get_or_insert_with(|| Resources { limits: Limits::default() });
    resources.limits.devices.insert(name.to_string(), count.to_string());
    if !g.capabilities.is_empty() {
        c.env.push(EnvVar::Value(NameValue {
            name: NVIDIA_CAPABILITIES.into(),
            value: g.capabilities.join(","),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpus(yaml: &str) -> Gpus {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn only_a_count_of_a_known_driver_has_a_kubernetes_form() {
        assert_eq!(why("a", &gpus("2")), None);
        assert_eq!(why("a", &gpus("{ driver: amd }")), None);
        for (spec, says) in [
            ("all", "`gpus: all`"),
            ("{ ids: ['0'] }", "`gpus.ids`"),
            ("{ options: { a: b } }", "`gpus.options`"),
            ("{ driver: acme }", "the `acme` driver"),
            ("{ driver: amd, capabilities: [compute] }", "`gpus.capabilities`"),
        ] {
            assert!(why("a", &gpus(spec)).is_some_and(|w| w.contains(says)), "{spec}");
        }
    }

    #[test]
    fn the_count_is_a_limit_and_the_capabilities_an_environment_variable() {
        let mut c = Container::new("a", crate::objects::Image::Tool("x".into()));
        apply(Some(&gpus("{ count: 2, capabilities: [compute, utility] }")), &mut c);
        let yaml = serde_yaml_ng::to_string(&c).unwrap();
        assert!(yaml.contains("env:\n- name: NVIDIA_DRIVER_CAPABILITIES\n  value: compute,utility\n"), "{yaml}");
        assert!(yaml.contains("resources:\n  limits:\n    nvidia.com/gpu: '2'\n"), "{yaml}");
    }
}
