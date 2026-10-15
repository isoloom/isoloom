//! Which targets a spec can run on. Derived, not declared: a target is possible when every
//! machine (except the access machine, which the runner supplies) has the implementation
//! that target needs. All-or-nothing: one machine without `docker:` rules out every Docker
//! target, because an environment missing a machine doesn't behave like the original.

use crate::model::{Shape, Spec, Target};

/// Machines lacking the implementation a shape needs (the access machine never counts).
#[must_use]
pub fn missing(spec: &Spec, shape: Shape) -> Vec<String> {
    spec.machines
        .iter()
        .filter(|(_, m)| !m.access)
        .filter(|(_, m)| match shape {
            Shape::Docker => m.docker.is_none(),
            Shape::Vm => m.vm.is_none(),
            Shape::Either => m.docker.is_none() && m.vm.is_none(),
            Shape::VmOrLxc => m.vm.is_none() && !m.docker.as_ref().is_some_and(|d| d.lxc),
            Shape::External => m.external.is_none(),
            // Cloud services aren't machines: see `derive`.
            Shape::Cloud => false,
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// Every target the implementations allow, narrowed by the spec's `targets:` when set.
#[must_use]
pub fn derive(spec: &Spec) -> Vec<Target> {
    // Cloud services run on their own target only; a spec without machines runs nowhere else.
    if spec.cloud.is_some() {
        return vec![Target::CloudServices];
    }
    if spec.machines.is_empty() {
        return Vec::new();
    }
    Target::ALL
        .into_iter()
        .filter(|t| *t != Target::CloudServices)
        .filter(|t| missing(spec, t.needs()).is_empty())
        .filter(|t| *t != Target::Hybrid || mixed(spec))
        .collect()
}

/// Hybrid is worth it only when the environment mixes both: a machine that can only be a VM
/// (Windows, say) and one that can be a container. Otherwise Docker or VMs alone run it.
#[must_use]
pub fn mixed(spec: &Spec) -> bool {
    let own = || spec.machines.values().filter(|m| !m.access);
    own().any(|m| m.docker.is_some()) && own().any(|m| m.docker.is_none() && m.vm.is_some())
}

/// The targets the generators should produce: derived, then narrowed by `targets:`.
#[must_use]
pub fn effective(spec: &Spec) -> Vec<Target> {
    let possible = derive(spec);
    match &spec.targets {
        Some(wanted) => possible.into_iter().filter(|t| wanted.contains(t)).collect(),
        None => possible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lab, model};

    /// The lab with these machines (each a YAML flow mapping, by name).
    fn machines(list: &[(&str, &str)]) -> Result<Spec, String> {
        let lines: Vec<String> = list.iter().map(|(name, m)| format!("  {name}: {m}")).collect();
        model(&lab(&format!("machines:\n{}\n", lines.join("\n"))))
    }

    const DOCKER: &str = "{ docker: { image: nginx } }";
    const VM: &str = "{ vm: { os: debian@12 } }";
    const BOTH: &str = "{ docker: { image: nginx }, vm: { os: debian@12 } }";
    const LXC: &str = "{ docker: { image: nginx, lxc: true } }";
    const ACCESS: &str = "{ access: true }";

    #[test]
    fn a_machine_is_missing_when_it_lacks_what_the_shape_needs() -> Result<(), String> {
        let spec = machines(&[("web", DOCKER), ("win", VM), ("ct", LXC), ("me", ACCESS)])?;
        assert_eq!(missing(&spec, Shape::Docker), ["win"], "the access machine never counts");
        assert_eq!(missing(&spec, Shape::Vm), ["web", "ct"]);
        assert_eq!(missing(&spec, Shape::Either), Vec::<String>::new());
        assert_eq!(missing(&spec, Shape::VmOrLxc), ["web"], "an LXC container or a VM");
        assert_eq!(missing(&spec, Shape::External), ["web", "win", "ct"]);
        assert_eq!(missing(&spec, Shape::Cloud), Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn containers_alone_run_on_the_docker_targets() -> Result<(), String> {
        use Target::{CloudDocker, Docker, DockerVm, Hosted, Kubernetes};
        assert_eq!(
            derive(&machines(&[("web", DOCKER), ("me", ACCESS)])?),
            [Docker, Hosted, DockerVm, CloudDocker, Kubernetes]
        );
        Ok(())
    }

    #[test]
    fn machines_with_both_run_everywhere_but_hybrid() -> Result<(), String> {
        use Target::{CloudDocker, CloudVm, Docker, DockerVm, Esxi, Hosted, Kubernetes, Proxmox, Vagrant};
        let spec = machines(&[("web", BOTH)])?;
        assert!(!mixed(&spec), "a machine that can be either mixes nothing");
        assert_eq!(
            derive(&spec),
            [Docker, Hosted, DockerVm, CloudDocker, Kubernetes, Vagrant, Proxmox, Esxi, CloudVm]
        );
        Ok(())
    }

    #[test]
    fn a_container_beside_a_vm_only_machine_runs_on_hybrid_alone() -> Result<(), String> {
        let spec = machines(&[("web", DOCKER), ("win", VM)])?;
        assert!(mixed(&spec));
        assert_eq!(derive(&spec), [Target::Hybrid]);
        assert!(!mixed(&machines(&[("web", DOCKER)])?), "containers alone mix nothing");
        Ok(())
    }

    #[test]
    fn cloud_services_run_on_their_own_target_and_no_machines_nowhere() -> Result<(), String> {
        let cloud = model("version: 1\nname: t\ncloud: { provider: aws, terraform: infra }\n")?;
        assert_eq!(derive(&cloud), [Target::CloudServices]);
        assert_eq!(derive(&model(&lab(""))?), Vec::<Target>::new());
        Ok(())
    }

    #[test]
    fn targets_narrow_what_the_machines_allow() -> Result<(), String> {
        let mut spec = machines(&[("web", DOCKER)])?;
        assert_eq!(effective(&spec), derive(&spec), "without `targets:`, every possible one");
        spec.targets = Some(vec![Target::Kubernetes, Target::Vagrant]);
        assert_eq!(effective(&spec), [Target::Kubernetes], "a wanted target the machines rule out stays out");
        Ok(())
    }
}
