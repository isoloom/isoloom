//! What a machine asks of the hardware under it (`gpus`, `devices`, `nested_virtualization`),
//! and what that requires of the backend, by the machine's shape on the target: a container
//! gets the host's GPUs and devices from its runtime, a VM gets GPUs passed through or sized in
//! and runs VMs of its own through its hypervisor. An existing host is used as it is.

use std::fmt;

use isoloom_spec::model::{Device, Gpus, Machine, Spec};

use crate::environment::MachineShape;
use crate::requirements::{Feature, Requirement};

/// A machine's hardware, as the spec asks for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hardware {
    /// Its GPUs, if it asks for any.
    pub gpus: Option<Gpus>,
    /// The host's devices it is given.
    pub devices: Vec<Device>,
    /// Whether it runs VMs of its own.
    pub nested_virtualization: bool,
}

impl Hardware {
    /// What machine `m` asks for.
    pub fn of(m: &Machine) -> Hardware {
        Hardware {
            gpus: m.gpus.clone(),
            devices: m.devices.clone(),
            nested_virtualization: m.nested_virtualization,
        }
    }
}

/// One hardware need of a machine, in its shape on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// A container asks for GPUs.
    ContainerGpus,
    /// A container asks for the host's devices.
    ContainerDevices,
    /// A container asks to run VMs inside.
    ContainerNested,
    /// A VM asks for GPUs.
    VmGpus,
    /// A VM asks for the host's devices.
    VmDevices,
    /// A VM asks to run VMs inside.
    VmNested,
}

impl Need {
    /// The capability a backend needs to meet it.
    pub fn feature(self) -> Feature {
        match self {
            Need::ContainerGpus => Feature::ContainerGpus,
            Need::ContainerDevices => Feature::ContainerDevices,
            Need::ContainerNested => Feature::ContainerNestedVirtualization,
            Need::VmGpus => Feature::VmGpus,
            Need::VmDevices => Feature::VmDevices,
            Need::VmNested => Feature::NestedVirtualization,
        }
    }

    /// Why a backend without the capability refuses it, for machine `machine`. Each message names
    /// the targets that lack it, since only those ever show it.
    pub fn reason(self, machine: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Need::ContainerGpus => write!(
                f,
                "machine `{machine}` asks for GPUs (`gpus`): its container runs on a VM Isoloom creates, which has no GPU (the `docker` and `kubernetes` targets give the host's)"
            ),
            Need::ContainerDevices => write!(
                f,
                "machine `{machine}` is given host devices (`devices`): only a container on your own Docker host reaches them (the `docker` target), not one in a VM Isoloom creates or a Kubernetes pod"
            ),
            Need::ContainerNested => write!(
                f,
                "machine `{machine}` runs VMs inside (`nested_virtualization`), which a container can't: it runs as a VM on the VM targets"
            ),
            Need::VmGpus => write!(
                f,
                "machine `{machine}` asks for GPUs (`gpus`): Vagrant passes no GPU to a local VM (Proxmox and the cloud targets do)"
            ),
            Need::VmDevices => write!(
                f,
                "machine `{machine}` is given host devices (`devices`): a VM has its own virtual devices, so only a container gets the host's (a GPU reaches a VM through `gpus`)"
            ),
            Need::VmNested => write!(
                f,
                "machine `{machine}` runs VMs inside (`nested_virtualization`): this target's VMs can't nest (Vagrant, Proxmox, AWS, Google Cloud and Oracle Cloud can)"
            ),
        }
    }
}

/// What the machines' hardware asks of the backend: one requirement per machine and need, in
/// machine order. `shape` is each machine's shape on the target; an existing or supplied
/// machine asks nothing (it is used as it is).
pub fn requirements(spec: &Spec, shape: impl Fn(&str) -> Option<MachineShape>) -> Vec<Requirement> {
    let mut out = Vec::new();
    for (name, m) in &spec.machines {
        let needs: &[(bool, Need, Need)] = &[
            (m.gpus.is_some(), Need::ContainerGpus, Need::VmGpus),
            (!m.devices.is_empty(), Need::ContainerDevices, Need::VmDevices),
            (m.nested_virtualization, Need::ContainerNested, Need::VmNested),
        ];
        for (_, container, vm) in needs.iter().filter(|n| n.0) {
            let need = match shape(name) {
                Some(MachineShape::Container) => *container,
                Some(MachineShape::Vm) => *vm,
                _ => continue,
            };
            out.push(Requirement::Hardware { machine: name.clone(), need });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        let source = isoloom_spec::parse(
            "version: 1\nname: hw\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines:\n  \
             a: { networks: { lan: 10 }, hardware: { gpus: 1, devices: [/dev/fuse] }, os: debian@12, app: { image: x, steps: [{ run: a.sh }] } }\n  \
             b: { networks: { lan: 11 }, hardware: { nested_virtualization: true }, os: debian@12, app: { steps: [{ run: a.sh }] } }\n",
        )
        .unwrap();
        crate::Lab::new(source).spec()
    }

    fn needs(shape: MachineShape) -> Vec<(String, Need)> {
        requirements(&spec(), |_| Some(shape))
            .into_iter()
            .map(|r| match r {
                Requirement::Hardware { machine, need } => (machine, need),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn each_need_follows_the_machine_shape() {
        let n = |m: &str, need| (m.to_string(), need);
        assert_eq!(
            needs(MachineShape::Container),
            [n("a", Need::ContainerGpus), n("a", Need::ContainerDevices), n("b", Need::ContainerNested)]
        );
        assert_eq!(needs(MachineShape::Vm), [n("a", Need::VmGpus), n("a", Need::VmDevices), n("b", Need::VmNested)]);
        assert_eq!(needs(MachineShape::External), []);
        assert!(Hardware::of(&spec().machines["b"]).nested_virtualization);
    }

    #[test]
    fn every_need_says_why_and_names_its_field() {
        for (need, field) in [
            (Need::ContainerGpus, "gpus"),
            (Need::ContainerDevices, "devices"),
            (Need::ContainerNested, "nested_virtualization"),
            (Need::VmGpus, "gpus"),
            (Need::VmDevices, "devices"),
            (Need::VmNested, "nested_virtualization"),
        ] {
            let r = Requirement::Hardware { machine: "a".into(), need };
            assert!(r.to_string().contains(&format!("(`{field}`)")), "{r}");
            assert_eq!(r.feature(), need.feature());
        }
    }
}
