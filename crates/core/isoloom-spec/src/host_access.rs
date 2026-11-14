//! What an environment reaches of the computer that runs it, beyond its own machines: folders
//! outside the project or written to, the container engine's socket, a privileged container, the
//! host's devices, files a VM boots from on the hypervisor's host, a network bridged to the host's
//! own. Each is a [`HostAccess`], found in the spec by [`of`]. An environment reaches none of them
//! unless whoever runs it allows each one ([`Grants`]): a spec that asks for one more is refused
//! ([`refused`]), naming the machine, the field and the access to allow.

use std::collections::BTreeSet;
use std::fmt;

use crate::model::{HostPath, Spec};
use crate::syntax::Code;
use crate::validate::Problem;

/// One way an environment reaches the computer that runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum HostAccess {
    /// A host folder outside the project folder (`../notes`, `~`, `/srv/share`).
    HostFolders,
    /// A host folder the machine writes in (`read_only: false`).
    WritableHostFolders,
    /// The container engine's socket (`/var/run/docker.sock`) as a host folder: whoever holds
    /// it runs containers as the engine, the computer's administrator.
    DockerSocket,
    /// A privileged container (`privileged: true` on a container): it reaches past its own
    /// isolation into the host's kernel.
    Privileged,
    /// The host's devices given to a machine (`hardware.devices`).
    Devices,
    /// Files of the hypervisor's host a VM boots from or serves (`vm.boot`'s `kernel`, `initrd`,
    /// `dtb`, `loader`, `nvram`, `network.tftp_root`).
    BootFiles,
    /// A network bridged to an interface of the host (`bridged`): its machines are on the host's
    /// own network.
    HostNetwork,
}

impl HostAccess {
    /// Every access, in the order a refusal lists them.
    pub const ALL: [HostAccess; 7] = [
        HostAccess::HostFolders,
        HostAccess::WritableHostFolders,
        HostAccess::DockerSocket,
        HostAccess::Privileged,
        HostAccess::Devices,
        HostAccess::BootFiles,
        HostAccess::HostNetwork,
    ];

    /// Its name, as a command line (`--allow-host <id>`) and a caller give it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            HostAccess::HostFolders => "host-folders",
            HostAccess::WritableHostFolders => "writable-host-folders",
            HostAccess::DockerSocket => "docker-socket",
            HostAccess::Privileged => "privileged",
            HostAccess::Devices => "devices",
            HostAccess::BootFiles => "boot-files",
            HostAccess::HostNetwork => "host-network",
        }
    }

    /// The access named `id` ([`HostAccess::id`]).
    #[must_use]
    pub fn parse(id: &str) -> Option<HostAccess> {
        HostAccess::ALL.into_iter().find(|a| a.id() == id)
    }

    /// What it lets the environment do on the computer that runs it, as whoever allows it reads
    /// it.
    #[must_use]
    pub fn meaning(self) -> &'static str {
        match self {
            HostAccess::HostFolders => "a folder of this computer outside the project is shared with a machine",
            HostAccess::WritableHostFolders => "a machine writes in a folder of this computer",
            HostAccess::DockerSocket => "a machine controls this computer's container engine, which runs as its administrator",
            HostAccess::Privileged => "a container runs privileged, with power over this computer's kernel",
            HostAccess::Devices => "a machine is given devices of this computer",
            HostAccess::BootFiles => "a VM boots from, or serves, files of the hypervisor's computer",
            HostAccess::HostNetwork => "machines join this computer's own network",
        }
    }
}

impl fmt::Display for HostAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// Where a spec asks for a [`HostAccess`]: the field (`machines.web.privileged`), and its machine
/// when a machine's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostReach {
    /// The access it needs.
    pub access: HostAccess,
    /// The machine that asks for it; none for a network's.
    pub machine: Option<String>,
    /// The field, in the spec's model (`machines.web.host_folders[0]`).
    pub at: String,
}

/// The accesses whoever runs an environment allows it: none by default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants(BTreeSet<HostAccess>);

impl Grants {
    /// No access: the default.
    #[must_use]
    pub fn none() -> Grants {
        Grants::default()
    }

    /// Each access in `accesses`.
    #[must_use]
    pub fn of(accesses: impl IntoIterator<Item = HostAccess>) -> Grants {
        Grants(accesses.into_iter().collect())
    }

    /// Whether `access` is allowed.
    #[must_use]
    pub fn allows(&self, access: HostAccess) -> bool {
        self.0.contains(&access)
    }

    /// The accesses allowed, in [`HostAccess::ALL`]'s order.
    pub fn iter(&self) -> impl Iterator<Item = HostAccess> + '_ {
        self.0.iter().copied()
    }
}

/// Whether a host folder's `from` names a container engine's socket (`/var/run/docker.sock`,
/// `~/.docker/run/docker.sock`, Podman's, Windows' `//./pipe/docker_engine`).
fn is_engine_socket(from: &str) -> bool {
    let last = from.rsplit(['/', '\\']).next().unwrap_or_default();
    std::path::Path::new(last).extension().is_some_and(|e| e.eq_ignore_ascii_case("sock")) || from.to_ascii_lowercase().contains("docker_engine")
}

/// Every place `spec` reaches the computer that runs it, in the order the spec gives them:
/// machines (their host folders, privileged container, devices, boot files), then networks.
#[must_use]
pub fn of(spec: &Spec) -> Vec<HostReach> {
    let mut out = Vec::new();
    for (name, m) in &spec.machines {
        let reach = |access: HostAccess, at: String| HostReach {
            access,
            machine: Some(name.clone()),
            at,
        };
        let at = format!("machines.{name}");
        for (i, f) in m.host_folders.iter().enumerate() {
            let at = format!("{at}.host_folders[{i}]");
            if is_engine_socket(&f.from) {
                out.push(reach(HostAccess::DockerSocket, format!("{at}.from")));
                continue;
            }
            let inside = matches!(f.host(), HostPath::Project(from) if crate::project_path::plain(from).is_ok());
            if !inside {
                out.push(reach(HostAccess::HostFolders, format!("{at}.from")));
            }
            if !f.read_only {
                out.push(reach(HostAccess::WritableHostFolders, format!("{at}.read_only")));
            }
        }
        if m.privileged && m.docker.is_some() {
            out.push(reach(HostAccess::Privileged, format!("{at}.privileged")));
        }
        for i in 0..m.devices.len() {
            out.push(reach(HostAccess::Devices, format!("{at}.devices[{i}]")));
        }
        if let Some(b) = m.vm.as_ref().and_then(|v| v.boot.as_ref()) {
            let files = [
                ("kernel", &b.kernel),
                ("initrd", &b.initrd),
                ("dtb", &b.dtb),
                ("loader", &b.loader),
                ("nvram", &b.nvram),
            ];
            for (field, _) in files.iter().filter(|(_, v)| v.is_some()) {
                out.push(reach(HostAccess::BootFiles, format!("{at}.vm.boot.{field}")));
            }
            if b.network.as_ref().is_some_and(|n| n.tftp_root.is_some()) {
                out.push(reach(HostAccess::BootFiles, format!("{at}.vm.boot.network.tftp_root")));
            }
        }
    }
    for (name, n) in &spec.networks {
        if n.bridged.is_some() {
            out.push(HostReach {
                access: HostAccess::HostNetwork,
                machine: None,
                at: format!("networks.{name}.bridged"),
            });
        }
    }
    out
}

impl HostReach {
    /// Why it is refused until its access is allowed, and how to allow it: a problem's message.
    #[must_use]
    pub fn refusal(&self) -> String {
        let whose = self
            .machine
            .as_deref()
            .map_or_else(|| "the environment".to_owned(), |m| format!("machine `{m}`"));
        let id = self.access.id();
        format!(
            "{whose} reaches the computer that runs it ({}): refused unless whoever runs it allows `{id}` (`--allow-host {id}`)",
            self.access.meaning()
        )
    }
}

/// A `refused` problem at each place `spec` reaches the computer that runs it in a way `grants`
/// doesn't allow: what it would do, and the access to allow.
#[must_use]
pub fn refused(spec: &Spec, grants: &Grants) -> Vec<Problem> {
    of(spec)
        .into_iter()
        .filter(|r| !grants.allows(r.access))
        .map(|r| Problem::new(Code::Refused, r.at.clone(), r.refusal()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lab, model};

    /// The accesses and fields of the spec of machines `machines`.
    fn reached(machines: &str) -> Vec<(HostAccess, String)> {
        let spec = model(&lab(machines)).expect("a spec");
        of(&spec).into_iter().map(|r| (r.access, r.at)).collect()
    }

    #[test]
    fn a_project_folder_read_only_reaches_nothing() {
        let m =
            "machines: { web: { networks: { lan: 10 }, docker: { image: nginx }, host_folders: [{ from: shared, path: /a }, { from: ./site/, path: /b }] } }\n";
        assert_eq!(reached(m), []);
    }

    #[test]
    fn every_way_out_of_the_machines_is_found_at_its_field() {
        let m = "machines:
  web:
    networks: { lan: 10 }
    docker: { image: nginx }
    privileged: true
    devices: [/dev/sda, /dev/fuse]
    host_folders:
      - { from: '~', path: /h }
      - { from: /var/run/docker.sock, path: /var/run/docker.sock }
      - { from: shared, path: /s, read_only: false }
      - { from: ../up, path: /u, read_only: false }
      - { from: 'C:\\share', path: /c }
";
        let at = |f: &str| format!("machines.web.{f}");
        assert_eq!(
            reached(m),
            [
                (HostAccess::HostFolders, at("host_folders[0].from")),
                (HostAccess::DockerSocket, at("host_folders[1].from")),
                (HostAccess::WritableHostFolders, at("host_folders[2].read_only")),
                (HostAccess::HostFolders, at("host_folders[3].from")),
                (HostAccess::WritableHostFolders, at("host_folders[3].read_only")),
                (HostAccess::HostFolders, at("host_folders[4].from")),
                (HostAccess::Privileged, at("privileged")),
                (HostAccess::Devices, at("devices[0]")),
                (HostAccess::Devices, at("devices[1]")),
            ]
        );
    }

    #[test]
    fn a_privileged_vm_reaches_nothing_but_its_own_kernel() {
        let m = "machines: { vm1: { networks: { lan: 10 }, vm: { os: debian@12 }, privileged: true } }\n";
        assert_eq!(reached(m), []);
    }

    #[test]
    fn a_vm_booting_from_the_hosts_files_and_a_bridged_network_reach_the_host() {
        let spec = model(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 }, home: { cidr: 192.168.1.0/24, bridged: { interface: en0 } } }\nmachines:\n  vm1:\n    networks: { lan: 20 }\n    vm: { os: debian@12, boot: { kernel: /boot/vmlinuz, initrd: /boot/initrd, order: [network, hd], network: { tftp_root: / } } }\n",
        )
        .expect("a spec");
        let found: Vec<(HostAccess, String)> = of(&spec).into_iter().map(|r| (r.access, r.at)).collect();
        assert_eq!(
            found,
            [
                (HostAccess::BootFiles, "machines.vm1.vm.boot.kernel".to_owned()),
                (HostAccess::BootFiles, "machines.vm1.vm.boot.initrd".to_owned()),
                (HostAccess::BootFiles, "machines.vm1.vm.boot.network.tftp_root".to_owned()),
                (HostAccess::HostNetwork, "networks.home.bridged".to_owned()),
            ]
        );
    }

    #[test]
    fn only_what_is_allowed_passes_and_the_rest_says_what_to_allow() {
        let spec = model(&lab(
            "machines: { web: { networks: { lan: 10 }, docker: { image: nginx }, privileged: true, devices: [/dev/fuse] } }\n",
        ))
        .expect("a spec");
        let lines = |g: &Grants| refused(&spec, g).iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            lines(&Grants::none()),
            [
                "machines.web.privileged: machine `web` reaches the computer that runs it (a container runs privileged, with power over this computer's kernel): refused unless whoever runs it allows `privileged` (`--allow-host privileged`)",
                "machines.web.devices[0]: machine `web` reaches the computer that runs it (a machine is given devices of this computer): refused unless whoever runs it allows `devices` (`--allow-host devices`)",
            ]
        );
        assert_eq!(lines(&Grants::of([HostAccess::Privileged])).len(), 1);
        assert_eq!(lines(&Grants::of([HostAccess::Privileged, HostAccess::Devices])), Vec::<String>::new());
    }

    #[test]
    fn every_access_is_named_once_and_read_back_by_its_name() {
        for a in HostAccess::ALL {
            assert_eq!(HostAccess::parse(a.id()), Some(a));
        }
        assert_eq!(HostAccess::parse("root"), None);
    }
}
