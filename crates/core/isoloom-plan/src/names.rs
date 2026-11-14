//! The names an environment takes on the host, where a provider shares them with every other
//! environment and every other copy of this one: its VMs, its networks, its libvirt domains.
//! Built in one place, from the environment's name (which carries the instance:
//! `isoloom_spec::instance::apply` names instance `n` of `web` `web-<n>`) and the copy's key,
//! which is known only when the copy runs: the one its runner gives in [`KEY_VAR`], else one the
//! Vagrantfile makes from its own folder. So two copies of one environment, numbered or not, never
//! share a name.

use isoloom_spec::contract::RESERVED_PREFIX;

/// The variable a Vagrantfile reads the copy's key from when it loads: what a runner sets for
/// every Vagrant command of a run, the same at each, so the run's names never change. Unset (a
/// copy run by hand), the Vagrantfile makes one from its folder's path.
pub const KEY_VAR: &str = "ISOLOOM_KEY";

/// How many characters a copy's key has: lowercase letters and digits (`[0-9a-z]{6}`); a
/// Vagrantfile refuses another key.
pub const KEY_LEN: usize = 6;

/// The host-visible names of one environment (one instance of it), as [`crate::Environment::names`]
/// gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Names<'a> {
    /// The environment's name, with its instance's suffix when it is one.
    environment: &'a str,
}

/// One name on the host: `<before>-<key><after>`, the copy's key after the environment's name, so
/// the names of one environment still sort and read together (`solo-web-k3x9a2 - web`,
/// `isoloom-solo-web-k3x9a2-app`), and `isoloom-<env>-` stays the prefix a clean-up matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostName {
    /// What comes before the key: the environment's name, or the reserved prefix and it.
    before: String,
    /// What comes after the key: the separator and the machine's or the network's name.
    after: String,
}

impl HostName {
    /// The name of the copy whose key is `key`.
    #[must_use]
    pub fn with_key(&self, key: &str) -> String {
        format!("{}-{key}{}", self.before, self.after)
    }

    /// The text before the key, its `-` included, and the text after it: what a Vagrantfile puts
    /// around the key it reads when it loads.
    #[must_use]
    pub fn around_key(&self) -> (String, &str) {
        (format!("{}-", self.before), &self.after)
    }

    /// The name with no key, as Isoloom 2 named every copy: a seed for what must stay the same
    /// in every copy (a Hyper-V adapter's MAC address), or a name that is the copy's own already
    /// (a Docker network inside the copy's own VM). Never a name the host shares between copies.
    #[must_use]
    pub fn unkeyed(&self) -> String {
        format!("{}{}", self.before, self.after)
    }

    /// Its longest form: with a key of [`KEY_LEN`] characters, as a provider's length
    /// limit sees it.
    #[must_use]
    pub fn len(&self) -> usize {
        self.before.len() + 1 + KEY_LEN + self.after.len()
    }

    /// Never: a name holds at least its key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl<'a> Names<'a> {
    /// The names of the environment named `environment` (with its instance's suffix).
    #[must_use]
    pub fn new(environment: &'a str) -> Self {
        Self { environment }
    }

    /// The environment's name, with its instance's suffix.
    #[must_use]
    pub fn environment(&self) -> &'a str {
        self.environment
    }

    fn after_environment(&self, after: String) -> HostName {
        HostName {
            before: self.environment.to_owned(),
            after,
        }
    }

    /// A VM's display name, as VirtualBox, VMware Desktop, Parallels and UTM show it:
    /// `<env>-<key> - <define>`, after its define name (an environment machine's, or one of
    /// Isoloom's own under `isoloom-`).
    #[must_use]
    pub fn vm_label(&self, define: &str) -> HostName {
        self.after_environment(format!(" - {define}"))
    }

    /// A VM's name on Hyper-V and ESXi, with no space: `<env>-<key>-<define>`.
    #[must_use]
    pub fn vm_name(&self, define: &str) -> HostName {
        self.after_environment(format!("-{define}"))
    }

    /// The prefix of the environment's libvirt domains, `<env>-<key>_` (libvirt otherwise takes
    /// the folder's name).
    #[must_use]
    pub fn libvirt_prefix(&self) -> HostName {
        self.after_environment("_".to_owned())
    }

    /// A VM's libvirt domain: `<env>-<key>_<define>`, its prefix then its define name.
    #[must_use]
    pub fn libvirt_domain(&self, define: &str) -> HostName {
        self.after_environment(format!("_{define}"))
    }

    /// A network on the host, `isoloom-<env>-<key>-<network>` (after [`RESERVED_PREFIX`]): its
    /// VirtualBox internal network, its libvirt network, its Hyper-V private switch.
    #[must_use]
    pub fn network(&self, network: &str) -> HostName {
        HostName {
            before: format!("{RESERVED_PREFIX}{}", self.environment),
            after: format!("-{network}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_holds_the_environment_its_instance_and_the_copy_key() {
        let plain = Names::new("solo-web");
        assert_eq!(plain.vm_label("web").with_key("k3x9a2"), "solo-web-k3x9a2 - web");
        assert_eq!(plain.vm_name("web").with_key("k3x9a2"), "solo-web-k3x9a2-web");
        assert_eq!(plain.libvirt_prefix().with_key("k3x9a2"), "solo-web-k3x9a2_");
        assert_eq!(plain.libvirt_domain("web").with_key("k3x9a2"), "solo-web-k3x9a2_web");
        assert_eq!(plain.network("app").with_key("k3x9a2"), "isoloom-solo-web-k3x9a2-app");
        let second = Names::new("solo-web-2");
        assert_eq!(second.vm_label("web").with_key("000001"), "solo-web-2-000001 - web");
        assert_eq!(second.network("app").with_key("000001"), "isoloom-solo-web-2-000001-app");
    }

    #[test]
    fn a_name_splits_around_its_key_and_keeps_isoloom_2s_form_unkeyed() {
        let label = Names::new("solo-web").vm_label("web");
        assert_eq!(label.around_key(), ("solo-web-".to_owned(), " - web"));
        assert_eq!(label.unkeyed(), "solo-web - web", "Isoloom 2's name");
        assert_eq!(label.len(), label.with_key("k3x9a2").len());
    }
}
