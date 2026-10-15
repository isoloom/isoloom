//! The image each target uses for an OS name: the built-in table, then the runner's own
//! table ([`Table`]: a file given to `isoloom generate --images`, or a tool's own settings),
//! then what a spec gives itself (`vm.image`).

use indexmap::IndexMap;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use crate::model::{DockerImpl, Spec, VmImage, VmImpl};

/// The runner's own image table, between the built-in one and the spec's `vm.image`:
///
/// ```yaml
/// os:                     # an OS's image, instead of the built-in one
///   debian@12: { vagrant: my-org/debian-12, vagrant_version: "1.2.0" }
/// access:                 # the access machine, when the spec leaves it to the runner
///   docker: kalilinux/kali-rolling
///   vm: kali
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// An OS's image per target, instead of the built-in one. Written as a spec writes the OS
    /// (`debian@12`, `debian`), held by the image table's name for it (`debian-12`).
    #[serde(default, deserialize_with = "os_keys")]
    pub os: IndexMap<String, VmImage>,
    /// The access machine, when the spec leaves it to the runner.
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
    /// An OS for VM targets, written as a spec writes it (`kali`, `debian@12`), held by the
    /// image table's name for it (its image from this table or the built-in one).
    #[serde(default, deserialize_with = "access_os")]
    pub vm: Option<String>,
}

/// The table's `os:` keys, each resolved to the image table's name for its OS.
fn os_keys<'de, D: Deserializer<'de>>(d: D) -> Result<IndexMap<String, VmImage>, D::Error> {
    IndexMap::<String, VmImage>::deserialize(d)?
        .into_iter()
        .map(|(key, image)| match crate::os::resolve(&key) {
            Ok(os) => Ok((os.name.to_string(), image)),
            Err(e) => Err(D::Error::custom(format!("os.{key}: {e}"))),
        })
        .collect()
}

/// The access machine's OS, resolved to the image table's name for it.
fn access_os<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|text| match crate::os::resolve(&text) {
            Ok(os) => Ok(os.name.to_string()),
            Err(e) => Err(D::Error::custom(e)),
        })
        .transpose()
}

impl Table {
    /// The table's image for an OS by its image-table name (`debian-12`).
    fn image_of(&self, os: &str) -> Option<&VmImage> {
        self.os.get(os)
    }

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
                && let Some(image) = self.image_of(&vm.os)
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
                    target: None,
                    args: IndexMap::new(),
                    init: Vec::new(),
                    idle: false,
                    appliance: None,
                    config: None,
                    firmware: None,
                    lxc: false,
                    generated: None,
                });
                m.supplied = true;
            }
            if m.vm.is_none()
                && let Some(os) = &access.vm
            {
                m.vm = Some(VmImpl {
                    image: self.image_of(os).cloned(),
                    os: os.clone(),
                    provision: Vec::new(),
                    tpm: None,
                    boot: None,
                    setup: Vec::new(),
                });
                m.supplied = true;
            }
        }
        out
    }
}

/// A Vagrant box: its name, a pinned version when known-good, and the libvirt, QEMU and Hyper-V
/// boxes when the main one has no build for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VagrantBox {
    /// The box's name on Vagrant Cloud (`bento/debian-12`).
    pub name: String,
    /// The version known to work (`None`: the latest).
    pub version: Option<String>,
    /// The box the libvirt provider boots, when the main one has no libvirt build.
    pub libvirt: Option<String>,
    /// The box the QEMU provider boots, when the main one has no QEMU build.
    pub qemu: Option<String>,
    /// The box the Hyper-V provider boots.
    pub hyperv: HyperVBox,
}

/// The box Vagrant's Hyper-V provider boots for a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyperVBox {
    /// The main box (it has a Hyper-V build at its version).
    Main,
    /// Another box, or another version of the main one (`None`: its latest).
    Other {
        /// The box's name.
        name: String,
        /// Its version (`None`: the latest).
        version: Option<String>,
    },
    /// No Hyper-V build known for this OS: give one with `vm.image.hyperv`.
    Missing,
}

/// Debian 12 on Hyper-V (bento's boxes no longer have Hyper-V builds; the roboxes `generic/*`
/// boxes do, their last version 4.3.12): Isoloom's own Debian VMs (router, tool shell) use it
/// there too.
pub const HYPERV_DEBIAN_12: (&str, &str) = ("generic/debian12", ROBOXES);

/// The last version of the roboxes `generic/*` boxes, which have Hyper-V builds.
const ROBOXES: &str = "4.3.12";

/// Debian 12 for QEMU: the official Debian cloud image as a box (amd64 and arm64), with the
/// cloud-init vagrant-qemu sets a private network's address through. Isoloom's own Debian VMs
/// (router, controller, tool shell) use it there too.
pub const QEMU_DEBIAN_12: &str = "cloud-image/debian-12";

/// Whether an OS name is a Windows one (WinRM and PowerShell instead of SSH and sh).
pub fn is_windows(os: &str) -> bool {
    os.starts_with("windows")
}

/// A box, its version when pinned, and its libvirt build when the main one has none.
fn boxed(name: &str, version: Option<&str>, libvirt: Option<&str>) -> VagrantBox {
    VagrantBox {
        name: name.into(),
        version: version.map(Into::into),
        libvirt: libvirt.map(Into::into),
        qemu: None,
        hyperv: HyperVBox::Main,
    }
}

/// `b` with its QEMU box (vagrant-qemu boots libvirt-format boxes), where the main one has none.
fn with_qemu(mut b: VagrantBox, qemu: &str) -> VagrantBox {
    b.qemu = Some(qemu.into());
    b
}

/// `b` with its Hyper-V box, where the main one has no Hyper-V build (bento's current boxes
/// have none).
fn with_hyperv(mut b: VagrantBox, name: &str, version: &str) -> VagrantBox {
    b.hyperv = HyperVBox::Other {
        name: name.into(),
        version: Some(version.into()),
    };
    b
}

/// `b`, with no Hyper-V build known anywhere.
fn no_hyperv(mut b: VagrantBox) -> VagrantBox {
    b.hyperv = HyperVBox::Missing;
    b
}

/// The built-in Vagrant boxes, by OS name.
fn builtin_vagrant(os: &str) -> Option<VagrantBox> {
    if is_windows(os) {
        return builtin_windows(os);
    }
    let (b, h) = (boxed, with_hyperv);
    Some(match os {
        "debian-11" => h(b("bento/debian-11", None, Some("generic/debian11")), "generic/debian11", ROBOXES),
        "debian-12" => h(
            with_qemu(b("bento/debian-12", None, Some("generic/debian12")), QEMU_DEBIAN_12),
            HYPERV_DEBIAN_12.0,
            HYPERV_DEBIAN_12.1,
        ),
        "debian-13" => no_hyperv(b("bento/debian-13", None, None)),
        // End of life: for labs about older systems (Metasploitable 3 runs on 14.04). No cloud
        // or Proxmox image is mapped for these, so those targets decline them.
        "ubuntu-14.04" => no_hyperv(b("ubuntu/trusty64", None, None)),
        "ubuntu-16.04" => h(b("bento/ubuntu-16.04", None, Some("generic/ubuntu1604")), "generic/ubuntu1604", ROBOXES),
        "ubuntu-18.04" => h(b("bento/ubuntu-18.04", None, Some("generic/ubuntu1804")), "generic/ubuntu1804", ROBOXES),
        "ubuntu-20.04" => h(b("bento/ubuntu-20.04", None, Some("generic/ubuntu2004")), "generic/ubuntu2004", ROBOXES),
        "ubuntu-22.04" => h(b("bento/ubuntu-22.04", None, Some("generic/ubuntu2204")), "generic/ubuntu2204", ROBOXES),
        // No roboxes build of 24.04: gusztavvargadr's, its last version with a Hyper-V build.
        "ubuntu-24.04" => h(b("bento/ubuntu-24.04", None, None), "gusztavvargadr/ubuntu-server-2404-lts", "2607.0.0"),
        "rocky-9" => h(b("bento/rockylinux-9", None, Some("generic/rocky9")), "generic/rocky9", ROBOXES),
        "almalinux-9" => h(b("bento/almalinux-9", None, Some("generic/alma9")), "generic/alma9", ROBOXES),
        // End of life (2024): for labs about older systems.
        "centos-7" => h(b("bento/centos-7", None, Some("generic/centos7")), "generic/centos7", ROBOXES),
        "fedora-42" => no_hyperv(b("bento/fedora-42", None, None)),
        // The box Isoloom's own controller boots (roboxes, with libvirt and Hyper-V builds).
        "alpine-3.19" => b("generic/alpine319", None, None),
        // Kali's own box has a Hyper-V build.
        "kali" => b("kalilinux/rolling", None, None),
        _ => return None,
    })
}

/// The built-in Windows boxes: each has a Hyper-V build at its pinned version, but
/// windows-server-2019's.
fn builtin_windows(os: &str) -> Option<VagrantBox> {
    let b = boxed;
    Some(match os {
        "windows-10" => b("gusztavvargadr/windows-10", Some("2511.0.0"), None),
        // End of life, evaluation boxes (Ansible's own test boxes), pinned: for labs about
        // older Windows. No cloud or Proxmox image is mapped for these.
        "windows-server-2008r2" => b("jborean93/WindowsServer2008R2", Some("0.7.0"), None),
        "windows-server-2012r2" => b("jborean93/WindowsServer2012R2", Some("1.2.0"), None),
        "windows-server-2016" => b("StefanScherer/windows_2016", Some("2019.02.14"), None),
        // The box GOAD uses (VirtualBox, VMware), pinned to its known-good version. On QEMU (an
        // Apple Silicon Mac emulating x86), an evaluation box built for libvirt. On Hyper-V, its
        // last version with a Hyper-V build.
        "windows-server-2019" => with_hyperv(
            with_qemu(
                b("StefanScherer/windows_2019", Some("2021.05.15"), None),
                "peru/windows-server-2019-standard-x64-eval",
            ),
            "StefanScherer/windows_2019",
            "2020.02.12",
        ),
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

/// The Vagrant box for a machine: the spec's `vm.image.vagrant` when set, else the built-in one;
/// on Hyper-V, the spec's `vm.image.hyperv` when set (its latest version with a Hyper-V build).
pub fn vagrant(vm: &VmImpl) -> Option<VagrantBox> {
    let image = vm.image.as_ref();
    let mut b = match image.and_then(|i| i.vagrant.clone()) {
        Some(name) => VagrantBox {
            name,
            version: image.and_then(|i| i.vagrant_version.clone()),
            libvirt: None,
            qemu: image.and_then(|i| i.qemu.clone()),
            hyperv: HyperVBox::Main,
        },
        None => builtin_vagrant(&vm.os)?,
    };
    if let Some(name) = image.and_then(|i| i.hyperv.clone()) {
        b.hyperv = HyperVBox::Other { name, version: None };
    }
    Some(b)
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

    #[test]
    fn hyperv_boxes_where_the_main_one_has_no_build() {
        let other = |os: &str| match builtin_vagrant(os).map(|b| b.hyperv) {
            Some(HyperVBox::Other { name, version }) => Some((name, version)),
            _ => None,
        };
        let pinned = |n: &str, v: &str| Some((n.to_string(), Some(v.to_string())));
        assert_eq!(other("debian-12"), pinned("generic/debian12", "4.3.12"));
        assert_eq!(other("ubuntu-24.04"), pinned("gusztavvargadr/ubuntu-server-2404-lts", "2607.0.0"));
        assert_eq!(other("windows-server-2019"), pinned("StefanScherer/windows_2019", "2020.02.12"));
        for os in ["kali", "windows-11", "windows-server-2022"] {
            assert_eq!(builtin_vagrant(os).map(|b| b.hyperv), Some(HyperVBox::Main), "{os}");
        }
        for os in ["debian-13", "ubuntu-14.04", "fedora-42"] {
            assert_eq!(builtin_vagrant(os).map(|b| b.hyperv), Some(HyperVBox::Missing), "{os}");
        }
    }

    #[test]
    fn the_specs_hyperv_box_wins() {
        let vm: VmImpl = serde_yaml_ng::from_str("os: debian-13\nimage: { hyperv: my/debian13 }\n").unwrap();
        let b = vagrant(&vm).unwrap();
        assert_eq!(b.name, "bento/debian-13");
        let mine = HyperVBox::Other {
            name: "my/debian13".into(),
            version: None,
        };
        assert_eq!(b.hyperv, mine);
    }
}
