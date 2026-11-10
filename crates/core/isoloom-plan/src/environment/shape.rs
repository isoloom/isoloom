//! What a machine is: how the target produces it, and what it is to the user.

use isoloom_spec::model::{Machine, Target};

/// How a machine is produced on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineShape {
    /// A container.
    Container,
    /// A virtual machine.
    Vm,
    /// An access machine the spec leaves to the runner with none of the target's forms from the
    /// image table: the target refuses the lab, so nothing renders it.
    Supplied,
    /// An existing machine, reached as it is (every machine on the `external` target).
    External,
}

/// What a machine is to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineRole {
    /// A machine of the environment.
    Member,
    /// The access machine (`access: true`): the one the user lands on and works from, started
    /// on demand.
    Access,
}

/// How the target produces machine `m`. Lowering keeps only the forms the target runs and
/// refuses a machine without one, so a machine with neither form here is the access machine the
/// spec leaves to the runner, which the target refuses unless the image table gave it its form
/// (`Lab::not_possible`).
pub(super) fn shape(target: Target, m: &Machine) -> MachineShape {
    if target == Target::External {
        return MachineShape::External;
    }
    let container = match target {
        Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => true,
        Target::Hybrid => m.docker.is_some(),
        // Proxmox runs a machine with no `vm:` as an LXC container when it opts in.
        Target::Proxmox => m.vm.is_none() && m.docker.as_ref().is_some_and(|d| d.lxc),
        _ => false,
    };
    match (container, m.docker.is_some(), m.vm.is_some()) {
        (true, true, _) => MachineShape::Container,
        (false, _, true) => MachineShape::Vm,
        _ => MachineShape::Supplied,
    }
}
