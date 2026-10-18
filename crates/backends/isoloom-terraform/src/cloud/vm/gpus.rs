//! A machine's GPUs on a cloud: the GPU comes with the instance type, so a count picks the
//! smallest of the cloud's GPU types that has as many (each cloud's table). The cloud picks the
//! GPUs, its types are NVIDIA's, and the guest's driver is the machine's own provisioning (the
//! stock images have none); what can't be met that way is refused with the reason.

use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::{GpuCount, Gpus};

use crate::cloud::Cloud;

/// A cloud's GPU instance types, by how many GPUs they have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GpuSizes {
    /// The GPU its types carry, as its refusals say it (`NVIDIA T4`).
    pub(crate) gpu: &'static str,
    /// Each type and its GPUs, fewest first. Empty: the cloud has none yet.
    pub(crate) sizes: &'static [(u32, &'static str)],
    /// Why there are none, when `sizes` is empty.
    pub(crate) none: &'static str,
}

impl GpuSizes {
    /// The instance type for a machine's GPUs: the smallest with at least as many. `None` when
    /// the machine asks for none (or more than the cloud has, which [`refusal`] refuses).
    pub(crate) fn of(&self, mp: &MachinePlan) -> Option<&'static str> {
        let n = mp.hardware.gpus.as_ref()?.fixed_count()?;
        self.sizes.iter().find(|(gpus, _)| *gpus >= n).map(|(_, size)| *size)
    }
}

/// Why `cloud` can't give the machines their GPUs from `sizes`, if it can't.
pub(crate) fn refusal(env: &Environment, cloud: Cloud, sizes: &GpuSizes) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .find_map(|(name, mp)| mp.hardware.gpus.as_ref().and_then(|g| why(name, g, cloud, sizes)))
}

fn why(name: &str, g: &Gpus, cloud: Cloud, s: &GpuSizes) -> Option<String> {
    let at = format!("machine `{name}`");
    let Some(&(most, largest)) = s.sizes.last() else {
        return Some(format!("{at}: {}", s.none));
    };
    if g.count == Some(GpuCount::All) || !g.ids.is_empty() {
        return Some(format!(
            "{at}: a {cloud} VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids`"
        ));
    }
    if !g.options.is_empty() {
        return Some(format!(
            "{at}: `gpus.options` are Docker's GPU driver options, which a {cloud} VM has no place for"
        ));
    }
    if g.driver() != isoloom_spec::model::DEFAULT_GPU_DRIVER {
        return Some(format!("{at}: {cloud}'s GPU instance types here carry NVIDIA GPUs, not `{}` ones", g.driver()));
    }
    g.fixed_count()
        .filter(|n| *n > most)
        .map(|n| format!("{at}: {n} GPUs; a {cloud} instance here has at most {most} ({largest}, {})", s.gpu))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SIZES: GpuSizes = GpuSizes {
        gpu: "NVIDIA T4",
        sizes: &[(1, "small"), (4, "large")],
        none: "",
    };

    fn gpus(v: serde_json::Value) -> Gpus {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_count_the_cloud_has_is_its_smallest_type() {
        assert_eq!(why("a", &gpus(json!(4)), Cloud::Aws, &SIZES), None);
        assert_eq!(why("a", &gpus(json!({ "capabilities": ["compute"] })), Cloud::Aws, &SIZES), None);
        for (spec, says) in [
            (json!("all"), "give `gpus` as a count"),
            (json!({ "ids": ["0"] }), "give `gpus` as a count"),
            (json!({ "options": { "a": "b" } }), "`gpus.options`"),
            (json!({ "driver": "amd" }), "not `amd` ones"),
            (json!(5), "at most 4 (large, NVIDIA T4)"),
        ] {
            assert!(why("a", &gpus(spec.clone()), Cloud::Aws, &SIZES).is_some_and(|w| w.contains(says)), "{spec}");
        }
        let none = GpuSizes {
            sizes: &[],
            none: "no GPUs here",
            ..SIZES
        };
        assert_eq!(why("a", &gpus(json!(1)), Cloud::Aws, &none).as_deref(), Some("machine `a`: no GPUs here"));
    }
}
