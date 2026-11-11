//! The Vagrant box each OS boots: the built-in table, the image table's or the spec's own box
//! (`vm.image.vagrant`) winning, and the boxes a provider boots instead of the main one
//! (libvirt, QEMU, Hyper-V) where the main one has no build for it.

mod choice;

use isoloom_plan::Vm;
use isoloom_spec::images::is_windows;

pub(crate) use choice::{ARM_HOST, Choice, MainBox, ProviderBox};

/// A Vagrant box: its name, a pinned version when known-good, and the libvirt, QEMU and Hyper-V
/// boxes when the main one has no build for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VagrantBox {
    /// The box's name on Vagrant Cloud (`bento/debian-12`).
    pub(crate) name: String,
    /// The version known to work (`None`: the latest).
    pub(crate) version: Option<String>,
    /// The box the libvirt provider boots, when the main one has no libvirt build.
    pub(crate) libvirt: Option<String>,
    /// The box the QEMU provider boots, when the main one has no QEMU build.
    pub(crate) qemu: Option<String>,
    /// The box the Hyper-V provider boots.
    pub(crate) hyperv: HyperVBox,
}

/// The box Vagrant's Hyper-V provider boots for a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HyperVBox {
    /// The main box (it has a Hyper-V build at its version).
    Main,
    /// Another box, or another version of the main one (`None`: its latest).
    Other {
        /// The box's name.
        name: String,
        /// Its version (`None`: the latest).
        version: Option<String>,
    },
    /// No Hyper-V build known for this OS: give one with
    /// `overrides.vagrant.machines.<m>.boxes.hyperv`.
    Missing,
}

/// Debian 12 on Hyper-V (bento's boxes no longer have Hyper-V builds; the roboxes `generic/*`
/// boxes do, their last version 4.3.12): Isoloom's own Debian VMs (router, tool shell) use it
/// there too.
pub(crate) const HYPERV_DEBIAN_12: (&str, &str) = ("generic/debian12", ROBOXES);

/// Debian 12's box, the built-in one, which Isoloom's own Debian VMs (router, tool shell, the
/// containers' host) boot too.
pub(crate) const DEBIAN_12: &str = "bento/debian-12";

/// Debian 12 for UTM (`vagrant_utm`), which bento's and roboxes' boxes have no build for: an
/// amd64 and an arm64 one. The containers' host boots it there.
pub(crate) const UTM_DEBIAN_12: &str = "utm/bookworm";

/// The last version of the roboxes `generic/*` boxes, which have Hyper-V builds.
const ROBOXES: &str = "4.3.12";

/// Debian 12 for QEMU: the official Debian cloud image as a box (amd64 and arm64), with the
/// cloud-init vagrant-qemu sets a private network's address through. Isoloom's own Debian VMs
/// (router, controller, tool shell) use it there too.
pub(crate) const QEMU_DEBIAN_12: &str = "cloud-image/debian-12";

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

/// Debian 12's built-in boxes: an environment machine's on `debian@12`, and the containers' host's.
pub(crate) fn debian_12() -> VagrantBox {
    with_hyperv(
        with_qemu(boxed(DEBIAN_12, None, Some("generic/debian12")), QEMU_DEBIAN_12),
        HYPERV_DEBIAN_12.0,
        HYPERV_DEBIAN_12.1,
    )
}

/// The built-in Vagrant boxes, by OS name.
fn builtin_vagrant(os: &str) -> Option<VagrantBox> {
    if is_windows(os) {
        return builtin_windows(os);
    }
    let (b, h) = (boxed, with_hyperv);
    Some(match os {
        "debian-11" => h(b("bento/debian-11", None, Some("generic/debian11")), "generic/debian11", ROBOXES),
        "debian-12" => debian_12(),
        "debian-13" => no_hyperv(b("bento/debian-13", None, None)),
        // End of life: for labs about older systems. No cloud or Proxmox image is mapped for
        // these, so those targets decline them.
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
        // A supported Alpine: the official cloud image as a box (VirtualBox, libvirt and QEMU
        // builds, none for Hyper-V).
        "alpine-3.24" => no_hyperv(b("cloud-image/alpine-3.24", None, None)),
        // Kali's own box has a Hyper-V build.
        "kali" => b("kalilinux/rolling", None, None),
        _ => return None,
    })
}

impl VagrantBox {
    /// The boxes one of Isoloom's own VMs boots when it boots this one: the computer's build on
    /// every provider, libvirt's and QEMU's own where it has them, `utm` on UTM; no Hyper-V block.
    pub(crate) fn own_vm<'a>(&'a self, utm: Option<&'a str>) -> Choice<'a> {
        Choice {
            main: MainBox {
                version: self.version.as_deref(),
                ..MainBox::plain(&self.name)
            },
            libvirt: self.libvirt.as_deref(),
            qemu: self.qemu.as_deref(),
            cpu: None,
            hyperv: None,
            esxi: None,
            utm,
        }
    }
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
        "windows-server-2016" => b("StefanScherer/windows_2016", Some("2019.02.14"), Some("jborean93/WindowsServer2016")),
        // A widely used community box (VirtualBox, VMware), pinned to its known-good version. On
        // QEMU (an Apple Silicon Mac emulating x86), an evaluation box built for libvirt. On
        // Hyper-V, its last version with a Hyper-V build.
        "windows-server-2019" => with_hyperv(
            with_qemu(
                b(
                    "StefanScherer/windows_2019",
                    Some("2021.05.15"),
                    Some("peru/windows-server-2019-standard-x64-eval"),
                ),
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

/// The Vagrant box for VM `vm`: its own image's `vagrant` when it has one, else the built-in one
/// for its OS; on Hyper-V, its own image's `hyperv` when set (its latest version with a Hyper-V
/// build).
#[must_use]
pub(crate) fn vagrant(vm: Vm) -> Option<VagrantBox> {
    let image = vm.plan.vm_image.as_ref();
    let mut b = match image.and_then(|i| i.vagrant.clone()) {
        Some(name) => VagrantBox {
            name,
            version: image.and_then(|i| i.vagrant_version.clone()),
            libvirt: None,
            qemu: image.and_then(|i| i.qemu.clone()),
            hyperv: HyperVBox::Main,
        },
        None => builtin_vagrant(vm.os)?,
    };
    if let Some(name) = image.and_then(|i| i.hyperv.clone()) {
        b.hyperv = HyperVBox::Other { name, version: None };
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::KNOWN_OS;

    use super::*;

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
        let pinned = |n: &str, v: &str| Some((n.to_owned(), Some(v.to_owned())));
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
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@13, networks: { lan: 10 } }\noverrides: { vagrant: { machines: { web: { boxes: { hyperv: my/debian13 } } } } }\n",
        );
        let b = vagrant(env.vm("web").unwrap()).unwrap();
        assert_eq!(b.name, "bento/debian-13");
        let mine = HyperVBox::Other {
            name: "my/debian13".into(),
            version: None,
        };
        assert_eq!(b.hyperv, mine);
    }
}
