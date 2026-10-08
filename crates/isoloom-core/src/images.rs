//! The image each target uses for an OS name: the built-in table, then the runner's own
//! table ([`Table`]: a file given to `isoloom generate --images`, or a launcher's settings),
//! then what a spec gives itself (`vm.image`).

use indexmap::IndexMap;
use serde::Deserialize;

use crate::model::{DockerImpl, Spec, VmImage, VmImpl};

/// The runner's own image table, between the built-in one and the spec's `vm.image`:
///
/// ```yaml
/// os:                     # an OS name's image, instead of the built-in one
///   debian-12: { vagrant: my-org/debian-12, vagrant_version: "1.2.0" }
/// access:                 # the access machine, when the spec leaves it to the runner
///   docker: kalilinux/kali-rolling
///   vm: kali
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    #[serde(default)]
    pub os: IndexMap<String, VmImage>,
    #[serde(default)]
    pub access: Option<Access>,
}

/// How the runner supplies an access machine that has no implementation in the spec.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    /// A container image (kept running idle) for container targets.
    #[serde(default)]
    pub docker: Option<String>,
    /// An OS name (from this table or the built-in one) for VM targets.
    #[serde(default)]
    pub vm: Option<String>,
}

impl Table {
    /// Reads a table from YAML. Unknown fields are errors.
    pub fn parse(yaml: &str) -> Result<Table, String> {
        serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
    }

    /// The spec with this table applied: each VM's image from `os:` (unless the spec gives its
    /// own `vm.image`), and the access machine's missing implementations from `access:`. The
    /// generators then treat a supplied machine like any other, on every target.
    pub fn apply(&self, spec: &Spec) -> Spec {
        let mut out = spec.clone();
        for m in out.machines.values_mut() {
            if let Some(vm) = &mut m.vm
                && vm.image.is_none()
                && let Some(image) = self.os.get(&vm.os)
            {
                vm.image = Some(image.clone());
            }
            let Some(access) = self.access.as_ref().filter(|_| m.access) else { continue };
            if m.docker.is_none()
                && let Some(image) = &access.docker
            {
                m.docker = Some(DockerImpl {
                    image: Some(image.clone()),
                    build: None,
                    dockerfile: None,
                    args: IndexMap::new(),
                    init: Vec::new(),
                    idle: false,
                    appliance: None,
                    config: None,
                    firmware: None,
                });
                m.supplied = true;
            }
            if m.vm.is_none()
                && let Some(os) = &access.vm
            {
                m.vm = Some(VmImpl {
                    os: os.clone(),
                    provision: Vec::new(),
                    image: self.os.get(os).cloned(),
                });
                m.supplied = true;
            }
        }
        out
    }
}

/// A Vagrant box: its name, a pinned version when known-good, and the libvirt box when the
/// main one has no libvirt build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VagrantBox {
    pub name: String,
    pub version: Option<String>,
    pub libvirt: Option<String>,
}

/// Whether an OS name is a Windows one (WinRM and PowerShell instead of SSH and sh).
pub fn is_windows(os: &str) -> bool {
    os.starts_with("windows")
}

/// The built-in Vagrant boxes, by OS name.
fn builtin_vagrant(os: &str) -> Option<VagrantBox> {
    let b = |name: &str, version: Option<&str>, libvirt: Option<&str>| VagrantBox {
        name: name.into(),
        version: version.map(Into::into),
        libvirt: libvirt.map(Into::into),
    };
    Some(match os {
        "debian-11" => b("bento/debian-11", None, Some("generic/debian11")),
        "debian-12" => b("bento/debian-12", None, Some("generic/debian12")),
        "debian-13" => b("bento/debian-13", None, None),
        // End of life: for labs about older systems (Metasploitable 3 runs on 14.04). No cloud
        // or Proxmox image is mapped for these, so those targets decline them.
        "ubuntu-14.04" => b("ubuntu/trusty64", None, None),
        "ubuntu-16.04" => b("bento/ubuntu-16.04", None, Some("generic/ubuntu1604")),
        "ubuntu-18.04" => b("bento/ubuntu-18.04", None, Some("generic/ubuntu1804")),
        "ubuntu-20.04" => b("bento/ubuntu-20.04", None, Some("generic/ubuntu2004")),
        "ubuntu-22.04" => b("bento/ubuntu-22.04", None, Some("generic/ubuntu2204")),
        "ubuntu-24.04" => b("bento/ubuntu-24.04", None, None),
        "rocky-9" => b("bento/rockylinux-9", None, Some("generic/rocky9")),
        "almalinux-9" => b("bento/almalinux-9", None, Some("generic/alma9")),
        // End of life (2024): for labs about older systems.
        "centos-7" => b("bento/centos-7", None, Some("generic/centos7")),
        "fedora-42" => b("bento/fedora-42", None, None),
        "kali" => b("kalilinux/rolling", None, None),
        "windows-10" => b("gusztavvargadr/windows-10", Some("2511.0.0"), None),
        // End of life, evaluation boxes (Ansible's own test boxes), pinned: for labs about
        // older Windows. No cloud or Proxmox image is mapped for these.
        "windows-server-2008r2" => b("jborean93/WindowsServer2008R2", Some("0.7.0"), None),
        "windows-server-2012r2" => b("jborean93/WindowsServer2012R2", Some("1.2.0"), None),
        "windows-server-2016" => b("StefanScherer/windows_2016", Some("2019.02.14"), None),
        // The box GOAD uses (VirtualBox, VMware, Hyper-V), pinned to its known-good version.
        "windows-server-2019" => b("StefanScherer/windows_2019", Some("2021.05.15"), None),
        // Pinned so a rebuilt box can't change an environment under it.
        "windows-server-2022" => b("gusztavvargadr/windows-server-2022-standard", Some("2607.0.0"), None),
        "windows-11" => b("gusztavvargadr/windows-11", Some("2607.1.0"), None),
        "windows-server-2025" => b("gusztavvargadr/windows-server-2025-standard", Some("2607.0.0"), None),
        _ => return None,
    })
}

/// How a machine's Windows box answers WinRM: what its image declares, else plain HTTP (every
/// built-in Windows box uses plain HTTP today).
pub fn winrm(vm: &VmImpl) -> crate::model::Winrm {
    vm.image.as_ref().and_then(|i| i.winrm).unwrap_or_default()
}

/// The Vagrant box for a machine: the spec's `vm.image.vagrant` when set, else the built-in one.
pub fn vagrant(vm: &VmImpl) -> Option<VagrantBox> {
    match vm.image.as_ref().and_then(|i| i.vagrant.clone()) {
        Some(name) => Some(VagrantBox {
            name,
            version: vm.image.as_ref().and_then(|i| i.vagrant_version.clone()),
            libvirt: None,
        }),
        None => builtin_vagrant(&vm.os),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KNOWN_OS;

    #[test]
    fn every_os_name_has_a_vagrant_box() {
        for os in KNOWN_OS {
            assert!(builtin_vagrant(os).is_some(), "no built-in Vagrant box for `{os}`");
        }
    }
}
