//! The controller VM the plan adds for `provision:` steps and Ansible checks: its size, and
//! whether it keeps running.

use isoloom_spec::model::Spec;

/// The controller VM, which runs the environment's playbooks and Ansible checks: its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Controller {
    /// CPUs: [`Controller::CPUS`] unless the spec says.
    pub cpus: u32,
    /// Memory in MB: [`Controller::MEMORY_MB`] unless the spec says.
    pub memory_mb: u32,
    /// Disk in GB, when the spec sets one (else its image's own).
    pub disk_gb: Option<u32>,
    /// Whether it stays on once the environment is built (`controller.keep_running`), rather
    /// than halting until the checks need it.
    pub keep_running: bool,
}

impl Controller {
    /// Its CPUs when the spec leaves them out.
    pub const CPUS: u32 = 1;
    /// Its memory in MB when the spec leaves it out: Python, ansible-core and pywinrm fit.
    pub const MEMORY_MB: u32 = 512;

    /// The controller of `spec` (`controller.resources`, else the defaults), whether or not
    /// the environment needs one.
    #[must_use]
    pub fn of(spec: &Spec) -> Controller {
        let r = spec.controller.as_ref().and_then(|c| c.resources.as_ref());
        Controller {
            cpus: r.and_then(|r| r.cpus).unwrap_or(Controller::CPUS),
            memory_mb: r.and_then(|r| r.memory_mb).unwrap_or(Controller::MEMORY_MB),
            disk_gb: r.and_then(|r| r.disk_gb),
            keep_running: spec.controller.as_ref().is_some_and(|c| c.keep_running),
        }
    }

    /// Whether `spec` needs a controller: it has `provision:` steps or Ansible checks.
    #[must_use]
    pub fn needed(spec: &Spec) -> bool {
        !spec.provision.is_empty() || spec.checks.iter().any(isoloom_spec::Check::is_playbook)
    }
}
