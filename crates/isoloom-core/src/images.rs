//! The image each target uses for an OS name, the built-in table, and the overrides a spec
//! can give (`vm.image`).

use crate::model::VmImpl;

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
