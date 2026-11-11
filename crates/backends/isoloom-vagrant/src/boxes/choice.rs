//! The box one VM boots under each provider, on a computer of each architecture: its main box
//! (`m.vm.box`), then a provider's own where that provider boots another (libvirt, QEMU, UTM,
//! Hyper-V, ESXi). Its Vagrantfile is written from it, and its run manifest lists it, so a runner
//! can download each box before `vagrant up` and the two never disagree.
//!
//! A box's build is the computer's architecture unless the VM pins one (`box_architecture`):
//! Vagrant 2.4 picks, among a box's versions, the newest with a build for the provider and that
//! architecture. On ESXi it is the ESXi host's ([`ESXI_ARCH`]), which the runner tells Vagrant.

use isoloom_manifest::Provider;
use isoloom_spec::model::Arch;

use super::HyperVBox;
use crate::ruby::{Ruby, Value};
use crate::vms::provider::ESXI_ARCH;

/// Whether the computer Vagrant runs on is an ARM one, in Ruby.
pub(crate) const ARM_HOST: &str = "RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/";

/// A VM's main box: the one every provider boots unless it has its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MainBox<'a> {
    /// Its name.
    pub(crate) name: &'a str,
    /// Its version, when pinned (`None`: the newest with the build).
    pub(crate) version: Option<&'a str>,
    /// The build it is pinned to (`None`: the computer's).
    pub(crate) arch: Option<Arch>,
    /// The box an arm64 computer boots instead, the main one having no arm64 build for the
    /// desktop hypervisors (the controller's Alpine).
    pub(crate) on_arm: Option<&'a str>,
}

impl<'a> MainBox<'a> {
    /// Box `name`, at its newest version, the computer's build.
    pub(crate) fn plain(name: &'a str) -> MainBox<'a> {
        MainBox {
            name,
            version: None,
            arch: None,
            on_arm: None,
        }
    }

    /// `<var>.vm.box` (on an arm64 computer, [`MainBox::on_arm`] when set), its version and its
    /// build when pinned. `name` is the box's name as Ruby, when it isn't a plain string (the
    /// access box, which the runner may give).
    pub(crate) fn render(&self, r: &mut Ruby, var: &str, name: Option<Value>) {
        let main = name.unwrap_or_else(|| Value::str(self.name));
        let named = match self.on_arm {
            Some(arm) => Value::raw(format!("{ARM_HOST} ? {} : {main}", Value::str(arm))),
            None => main,
        };
        r.assign(&format!("{var}.vm.box"), named);
        if let Some(v) = self.version {
            r.assign(&format!("{var}.vm.box_version"), Value::str(v));
        }
        if let Some(a) = self.arch {
            r.assign(&format!("{var}.vm.box_architecture"), Value::str(a.id()));
        }
    }
}

/// The boxes one VM boots, provider by provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Choice<'a> {
    /// The main box.
    pub(crate) main: MainBox<'a>,
    /// libvirt's own box, at its newest version, when it boots another.
    pub(crate) libvirt: Option<&'a str>,
    /// QEMU's own box, at its newest version, when it boots another.
    pub(crate) qemu: Option<&'a str>,
    /// The build QEMU boots, and libvirt its own box's: an environment machine's CPU (`None`
    /// for Isoloom's own VMs: the computer's).
    pub(crate) cpu: Option<Arch>,
    /// Hyper-V's box, when the VM has a Hyper-V block.
    pub(crate) hyperv: Option<&'a HyperVBox>,
    /// ESXi's own box, when it boots another (the main one's version pin kept).
    pub(crate) esxi: Option<&'a str>,
    /// UTM's own box, at its newest version, when it boots another.
    pub(crate) utm: Option<&'a str>,
}

/// One box a VM boots under one provider: what a runner downloads first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderBox {
    /// The box's name.
    pub(crate) name: String,
    /// Its version, when pinned.
    pub(crate) version: Option<String>,
    /// Its build, when pinned (`None`: the computer's).
    pub(crate) arch: Option<Arch>,
    /// Only on a computer of this architecture (`None`: any).
    pub(crate) host: Option<Arch>,
}

impl ProviderBox {
    fn of(name: &str, version: Option<&str>, arch: Option<Arch>) -> ProviderBox {
        ProviderBox {
            name: name.to_owned(),
            version: version.map(str::to_owned),
            arch,
            host: None,
        }
    }
}

impl Choice<'_> {
    /// The boxes the VM boots under `provider`: one, or one per architecture of the computer
    /// when the main box depends on it ([`MainBox::on_arm`]); none on a provider it has no box
    /// for (Hyper-V without a known build).
    pub(crate) fn on(&self, provider: Provider) -> Vec<ProviderBox> {
        let own = |name: &str, arch| vec![ProviderBox::of(name, None, arch)];
        match provider {
            Provider::Libvirt => self.libvirt.map_or_else(|| self.main(self.main.arch), |b| own(b, self.cpu)),
            Provider::Qemu => self.qemu.map_or_else(|| self.main(self.cpu), |b| own(b, self.cpu)),
            Provider::Utm => self.utm.map_or_else(|| self.main(self.main.arch), |b| own(b, None)),
            Provider::VmwareEsxi => {
                let name = self.esxi.unwrap_or(self.main.name);
                vec![ProviderBox::of(name, self.main.version, Some(ESXI_ARCH))]
            }
            Provider::Hyperv => match self.hyperv {
                None | Some(HyperVBox::Missing) => Vec::new(),
                Some(HyperVBox::Main) => self.main(self.main.arch),
                Some(HyperVBox::Other { name, version }) => vec![ProviderBox::of(name, version.as_deref(), self.main.arch)],
            },
            Provider::Virtualbox | Provider::VmwareDesktop | Provider::Parallels => self.main(self.main.arch),
        }
    }

    /// The main box, its build `arch`: on an arm64 computer, [`MainBox::on_arm`] when set.
    fn main(&self, arch: Option<Arch>) -> Vec<ProviderBox> {
        let main = ProviderBox::of(self.main.name, self.main.version, arch);
        match self.main.on_arm {
            None => vec![main],
            Some(arm) => vec![
                ProviderBox {
                    host: Some(Arch::Amd64),
                    ..main
                },
                ProviderBox {
                    host: Some(Arch::Arm64),
                    ..ProviderBox::of(arm, None, arch)
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arm_computer_boots_its_own_main_box_when_there_is_one() {
        let main = MainBox {
            on_arm: Some("bento/debian-12"),
            ..MainBox::plain("generic/alpine319")
        };
        let mut r = Ruby::new();
        main.render(&mut r, "m", None);
        assert_eq!(r.finish(), format!("m.vm.box = {ARM_HOST} ? \"bento/debian-12\" : \"generic/alpine319\"\n"));
        let mut pinned_ruby = Ruby::new();
        let pinned = MainBox {
            version: Some("1.0"),
            arch: Some(Arch::Arm64),
            ..MainBox::plain("bento/debian-12")
        };
        pinned.render(&mut pinned_ruby, "config", None);
        assert_eq!(
            pinned_ruby.finish(),
            "config.vm.box = \"bento/debian-12\"\nconfig.vm.box_version = \"1.0\"\nconfig.vm.box_architecture = \"arm64\"\n"
        );
    }

    const MAIN: MainBox<'static> = MainBox {
        name: "bento/debian-12",
        version: Some("1.0"),
        arch: None,
        on_arm: None,
    };

    fn choice() -> Choice<'static> {
        Choice {
            main: MAIN,
            libvirt: None,
            qemu: None,
            cpu: None,
            hyperv: None,
            esxi: None,
            utm: None,
        }
    }

    /// A listed box: its name, version, build and the computer it is for.
    type Listed = (String, Option<String>, Option<Arch>, Option<Arch>);

    fn names(c: &Choice, p: Provider) -> Vec<Listed> {
        c.on(p).into_iter().map(|b| (b.name, b.version, b.arch, b.host)).collect()
    }

    #[test]
    fn a_provider_boots_its_own_box_at_its_newest_version_else_the_main_one() {
        let c = Choice {
            libvirt: Some("generic/debian12"),
            qemu: Some("cloud-image/debian-12"),
            utm: Some("utm/bookworm"),
            ..choice()
        };
        let one = |n: &str, v: Option<&str>, a| vec![(n.to_owned(), v.map(str::to_owned), a, None)];
        assert_eq!(names(&c, Provider::Virtualbox), one("bento/debian-12", Some("1.0"), None));
        assert_eq!(names(&c, Provider::Libvirt), one("generic/debian12", None, None));
        assert_eq!(names(&c, Provider::Qemu), one("cloud-image/debian-12", None, None));
        assert_eq!(names(&c, Provider::Utm), one("utm/bookworm", None, None));
        // ESXi is x86-64, whatever runs Vagrant.
        assert_eq!(names(&c, Provider::VmwareEsxi), one("bento/debian-12", Some("1.0"), Some(Arch::Amd64)));
        assert_eq!(c.on(Provider::Hyperv), Vec::new(), "no Hyper-V block");
    }

    #[test]
    fn a_machines_cpu_pins_qemus_and_libvirts_own_builds() {
        let c = Choice {
            libvirt: Some("generic/debian12"),
            cpu: Some(Arch::Arm64),
            ..choice()
        };
        assert_eq!(c.on(Provider::Libvirt)[0].arch, Some(Arch::Arm64));
        assert_eq!(c.on(Provider::Qemu)[0].arch, Some(Arch::Arm64));
        assert_eq!(c.on(Provider::Virtualbox)[0].arch, None, "the main box's own pin: none");
    }

    #[test]
    fn a_main_box_for_arm_computers_is_listed_for_each_architecture() {
        let c = Choice {
            main: MainBox {
                version: None,
                on_arm: Some("bento/debian-12"),
                ..MainBox::plain("generic/alpine319")
            },
            ..choice()
        };
        let hosts: Vec<_> = c.on(Provider::Parallels).into_iter().map(|b| (b.name, b.host)).collect();
        assert_eq!(
            hosts,
            [
                ("generic/alpine319".to_owned(), Some(Arch::Amd64)),
                ("bento/debian-12".to_owned(), Some(Arch::Arm64))
            ]
        );
    }

    #[test]
    fn hyperv_takes_its_own_box_or_none() {
        let other = HyperVBox::Other {
            name: "generic/debian12".into(),
            version: Some("4.3.12".into()),
        };
        let c = Choice {
            hyperv: Some(&other),
            ..choice()
        };
        assert_eq!(
            names(&c, Provider::Hyperv),
            [("generic/debian12".to_owned(), Some("4.3.12".to_owned()), None, None)]
        );
        let missing = HyperVBox::Missing;
        let none = Choice {
            hyperv: Some(&missing),
            ..choice()
        };
        assert_eq!(none.on(Provider::Hyperv), Vec::new());
    }
}
