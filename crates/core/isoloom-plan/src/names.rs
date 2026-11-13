//! The names an environment takes on the host, where a provider shares them with every other
//! environment and every other copy of this one: its VMs, its networks, its libvirt domains.
//! Built in one place from the environment's name, which already carries the instance
//! (`isoloom_spec::instance::apply` names instance `n` of `web` `web-<n>`), so two instances of
//! one spec never share one, and an environment run without an instance keeps the names it
//! always had.

use isoloom_manifest::contract::RESERVED_PREFIX;

/// The host-visible names of one environment (one instance of it), as [`crate::Environment::names`]
/// gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Names<'a> {
    /// The environment's name, with its instance's suffix when it is one.
    environment: &'a str,
}

impl<'a> Names<'a> {
    /// The names of the environment named `environment` (with its instance's suffix).
    #[must_use]
    pub fn new(environment: &'a str) -> Self {
        Self { environment }
    }

    /// The environment's name, with its instance's suffix: the key every other name holds.
    #[must_use]
    pub fn environment(&self) -> &'a str {
        self.environment
    }

    /// A VM's display name, as VirtualBox, VMware Desktop, Parallels and UTM show it:
    /// `<env> - <define>`, after its define name (an environment machine's, or one of Isoloom's
    /// own under `isoloom-`).
    #[must_use]
    pub fn vm_label(&self, define: &str) -> String {
        format!("{} - {define}", self.environment)
    }

    /// A VM's name on Hyper-V and ESXi, with no space: `<env>-<define>`.
    #[must_use]
    pub fn vm_name(&self, define: &str) -> String {
        format!("{}-{define}", self.environment)
    }

    /// The prefix of the environment's libvirt domains, `<env>_` (libvirt otherwise takes the
    /// folder's name).
    #[must_use]
    pub fn libvirt_prefix(&self) -> String {
        format!("{}_", self.environment)
    }

    /// A VM's libvirt domain: `<env>_<define>`, its prefix then its define name.
    #[must_use]
    pub fn libvirt_domain(&self, define: &str) -> String {
        format!("{}{define}", self.libvirt_prefix())
    }

    /// A network on the host, `isoloom-<env>-<network>` (after [`RESERVED_PREFIX`]): its
    /// VirtualBox internal network, its libvirt network, its Hyper-V private switch, and the
    /// hybrid Docker host's network for it.
    #[must_use]
    pub fn network(&self, network: &str) -> String {
        format!("{RESERVED_PREFIX}{}-{network}", self.environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_keyed_by_the_environment_and_its_instance() {
        let plain = Names::new("solo-web");
        assert_eq!(plain.vm_label("web"), "solo-web - web");
        assert_eq!(plain.vm_name("web"), "solo-web-web");
        assert_eq!(plain.libvirt_prefix(), "solo-web_");
        assert_eq!(plain.libvirt_domain("web"), "solo-web_web");
        assert_eq!(plain.network("app"), "isoloom-solo-web-app");
        let second = Names::new("solo-web-2");
        assert_eq!(second.vm_label("web"), "solo-web-2 - web");
        assert_eq!(second.vm_name("web"), "solo-web-2-web");
        assert_eq!(second.libvirt_domain("web"), "solo-web-2_web");
        assert_eq!(second.network("app"), "isoloom-solo-web-2-app");
    }
}
