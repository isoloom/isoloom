//! The CM tools and how each infrastructure tool hands off to them (design §2.4): the container targets bake
//! the recipe into the image; Vagrant runs shell (PowerShell on Windows) or Ansible from the
//! controller; Terraform's targets carry cloud-init (PowerShell user data on Windows), shell or
//! Ansible; existing hosts get Ansible or shell. Each target's default is today's carrier, so
//! choosing nothing changes no output.

use std::fmt;
use std::str::FromStr;

use isoloom_spec::model::{Shape, Target};

use super::family::Family;
use crate::error::{Error, Result};

/// The CM tool that writes a VM's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Configure {
    /// The shell library (`configure.sh`), or the PowerShell library on Windows.
    Shell,
    /// The shell library carried in cloud-init's user data, run at first boot.
    CloudInit,
    /// Ansible, from Isoloom's Linux controller: `site.yml` and its roles.
    Ansible,
}

impl Configure {
    /// Every tool, in the order `--configure` lists them.
    pub const ALL: [Configure; 3] = [Configure::Shell, Configure::CloudInit, Configure::Ansible];

    /// How `--configure` and `overrides.<target>.configure` write it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Configure::Shell => "shell",
            Configure::CloudInit => "cloud-init",
            Configure::Ansible => "ansible",
        }
    }

    /// The tool `target` configures a VM of `family` with when none is chosen: today's carrier.
    /// `None` for a target whose machines are containers (the recipe is baked into the image)
    /// or cloud services.
    #[must_use]
    pub fn default_on(target: Target, family: Family) -> Option<Configure> {
        match target.needs() {
            Shape::Docker | Shape::Cloud => None,
            Shape::External => Some(Configure::Ansible),
            Shape::Vm | Shape::VmOrLxc if target != Target::Vagrant && family != Family::Windows => Some(Configure::CloudInit),
            Shape::Vm | Shape::VmOrLxc | Shape::Either => Some(Configure::Shell),
        }
    }

    /// Whether `target` can hand its VMs to this tool, else why not.
    ///
    /// # Errors
    ///
    /// [`Error::Unpaired`]: the container targets bake the recipe into the image, Vagrant's
    /// cloud-init is experimental and its boxes don't ship it, existing hosts never boot for it.
    pub fn pairs_with(self, target: Target) -> Result<()> {
        let why = match (target.needs(), self) {
            (Shape::Docker, _) => "a container target bakes the recipe into its image: `--configure` doesn't apply",
            (Shape::Cloud, _) => "cloud services have no machines to configure",
            (_, Configure::CloudInit) if matches!(target, Target::Vagrant | Target::Hybrid) => {
                "Vagrant's cloud-init is experimental and VirtualBox-only, and boxes don't ship cloud-init"
            }
            (Shape::External, Configure::CloudInit) => "existing hosts don't boot for cloud-init: it runs at a machine's first boot",
            _ => return Ok(()),
        };
        Err(Error::Unpaired { tool: self, target, why })
    }
}

impl fmt::Display for Configure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for Configure {
    type Err = Error;

    /// The tool `id` names (`cloud-init`).
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTool`], naming the tools.
    fn from_str(id: &str) -> Result<Configure> {
        Configure::ALL
            .into_iter()
            .find(|c| c.id() == id)
            .ok_or_else(|| Error::UnknownTool(id.to_owned()))
    }
}

/// A generated file's path, relative to the project folder (`.isoloom/docker/build/web/Dockerfile`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutPath(String);

impl OutPath {
    /// The path, when it is relative and stays inside the project folder.
    #[must_use]
    pub fn new(path: &str) -> Option<OutPath> {
        let inside = !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|p| p == ".." || p.is_empty());
        inside.then(|| OutPath(path.to_owned()))
    }

    /// The path as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the infrastructure tool reaches a machine's configuration: the file the CM renderer wrote, and who
/// runs it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Handoff {
    /// Baked into the container's image: the engine builds this Dockerfile.
    Image {
        /// The Dockerfile.
        dockerfile: OutPath,
    },
    /// Run inline by the infrastructure tool's own provisioner (Vagrant's `shell` or `powershell`), or by
    /// the runner's `ssh.run`/`winrm.run` after `apply`.
    Inline {
        /// The script.
        script: OutPath,
    },
    /// Read at first boot from the machine's user data (cloud-init, cloudbase-init).
    UserData {
        /// The cloud-config document.
        document: OutPath,
    },
    /// Run by Isoloom's controller once every machine is up.
    Controller {
        /// The playbook (`site.yml`).
        playbook: OutPath,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_target_defaults_to_todays_carrier() {
        assert_eq!(Configure::default_on(Target::Docker, Family::Debian), None);
        assert_eq!(Configure::default_on(Target::Vagrant, Family::Debian), Some(Configure::Shell));
        assert_eq!(Configure::default_on(Target::Vagrant, Family::Windows), Some(Configure::Shell));
        assert_eq!(Configure::default_on(Target::Proxmox, Family::RedHat), Some(Configure::CloudInit));
        assert_eq!(Configure::default_on(Target::CloudVm, Family::Windows), Some(Configure::Shell));
        assert_eq!(Configure::default_on(Target::External, Family::Debian), Some(Configure::Ansible));
        assert_eq!(Configure::default_on(Target::Hybrid, Family::Debian), Some(Configure::Shell));
    }

    #[test]
    fn a_tool_a_target_cant_hand_off_to_is_refused_with_why() {
        let e = Configure::CloudInit.pairs_with(Target::Vagrant).unwrap_err();
        assert!(
            e.to_string()
                .starts_with("vagrant: `--configure cloud-init`: Vagrant's cloud-init is experimental"),
            "{e}"
        );
        assert!(Configure::Ansible.pairs_with(Target::Docker).is_err() && Configure::CloudInit.pairs_with(Target::External).is_err());
        for target in [Target::Proxmox, Target::Esxi, Target::CloudVm] {
            assert!(Configure::ALL.into_iter().all(|c| c.pairs_with(target).is_ok()), "{target:?}");
        }
        assert!(Configure::Ansible.pairs_with(Target::Vagrant).is_ok() && Configure::Shell.pairs_with(Target::External).is_ok());
    }

    #[test]
    fn a_tool_reads_back_as_itself() {
        for c in Configure::ALL {
            assert_eq!(c.id().parse::<Configure>().ok(), Some(c));
        }
        assert!(
            "puppet"
                .parse::<Configure>()
                .unwrap_err()
                .to_string()
                .contains("`shell`, `cloud-init` or `ansible`")
        );
    }

    #[test]
    fn a_generated_path_stays_in_the_project() {
        assert_eq!(
            OutPath::new(".isoloom/docker/build/web/Dockerfile").map(|p| p.to_string()).as_deref(),
            Some(".isoloom/docker/build/web/Dockerfile")
        );
        for bad in ["", "/etc/x", "../x", "a//b"] {
            assert_eq!(OutPath::new(bad), None, "{bad}");
        }
    }
}
