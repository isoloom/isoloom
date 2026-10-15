//! How each kind of target produces a machine: the `docker:` and `vm:` blocks, appliances,
//! images, and the provisioning a VM runs.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Resources;

/// How a container produces a machine: an image, or one built from the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DockerImpl {
    /// A published image (exactly one of `image` and `build`).
    #[serde(default)]
    pub image: Option<String>,
    /// A folder of the project, built into the machine's image as its build context (exactly
    /// one of `image` and `build`).
    #[serde(default)]
    pub build: Option<String>,
    /// The Dockerfile, a file anywhere in the project (default: `Dockerfile` in `build`): an
    /// image of your own around a vendored project's source, without moving that source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    /// The stage of a multi-stage Dockerfile to build (`FROM ... AS <target>`), when not its
    /// last: the lab's image beside the release one, from one Dockerfile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Build arguments (`ARG`s of the Dockerfile), fixed values.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub args: IndexMap<String, String>,
    /// One-shot jobs (scripts or folders in the project) run before the machine counts as ready.
    #[serde(default)]
    pub init: Vec<String>,
    /// The image runs no service of its own (a stock Linux image whose command is a shell that
    /// exits at once): keep the container running idle, as a machine to work from.
    #[serde(default)]
    pub idle: bool,
    /// The image is a network appliance Isoloom knows how to wire (a router or switch OS in a
    /// container): its management port, interface order and startup configuration are set the
    /// way the image expects, its addresses go into that configuration. Docker only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appliance: Option<Appliance>,
    /// An appliance's own configuration (a file in the project), added to the one Isoloom
    /// generates (hostname, credentials, interfaces and their addresses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// The firmware an emulator boots (a file in the project): the IOS `.bin` of a Dynamips
    /// router. Isoloom builds the emulator's container itself, so no `image` then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// On Proxmox, a machine with no `vm:` may run as an LXC container from its `image` (an
    /// OCI image, Proxmox VE 9.1 or later) instead of ruling the `proxmox` target out. Opt-in:
    /// an LXC container is no Docker container (no Docker runtime, no healthcheck, no named
    /// volumes), so the author says the image runs as one. Ignored by every other target.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lxc: bool,
    /// The image Isoloom builds from the machine's portable steps (`app.steps`): a Dockerfile it
    /// generates, `FROM` the OS's official image. Set by lowering, never written in a spec.
    #[serde(skip)]
    #[schemars(skip)]
    pub generated: Option<Generated>,
}

/// A container image Isoloom builds from a machine's portable steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// The OS's image-table name (`debian-12`): its package installer.
    pub os: String,
    /// The OS's official image, pinned by digest: the `FROM`.
    pub base: String,
    /// The steps, in order.
    pub steps: Vec<crate::syntax::Step>,
}

/// A network appliance image (see [`DockerImpl::appliance`]). The images aren't Isoloom's to
/// ship: the user builds them from software they are licensed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Appliance {
    /// Cisco IOL (IOS on Linux, from Cisco Modeling Labs) as vrnetlab packages it
    /// (`vrnetlab/cisco_iol:<version>`): a router.
    CiscoIol,
    /// Cisco IOL-L2 (`vrnetlab/cisco_iol:L2-<version>`): a switch.
    CiscoIolL2,
    /// Cisco IOSv in QEMU (`vrnetlab/cisco_vios:<version>`, needs /dev/kvm): a router.
    CiscoVios,
    /// Cisco IOSvL2 in QEMU (`vrnetlab/cisco_viosl2:<version>`, needs /dev/kvm): a switch.
    CiscoViosL2,
    /// Cisco CSR1000v (IOS XE) in QEMU (`vrnetlab/cisco_csr1000v:<version>`, needs /dev/kvm).
    CiscoCsr1000v,
    /// Cisco Catalyst 8000v (IOS XE) in QEMU (`vrnetlab/cisco_c8000v:<version>`, needs /dev/kvm).
    CiscoC8000v,
    /// A classic Cisco 7200 router emulated by Dynamips, booting the IOS `.bin` in `firmware`.
    CiscoDynamips,
}

/// How a VM produces a machine: its OS, and the steps that set it up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VmImpl {
    /// The OS, as `<family>@<version>` (`debian@12`) or a family alone for its pinned default;
    /// each target maps it to an image. Once parsed, it holds the image table's name for the OS
    /// ([`KNOWN_OS`], see [`crate::os::canonical`]). Defaults to the machine's
    /// `os`, which it must agree with when both are set. May come from `common:` or a group
    /// (`vm: {}` in the machine).
    #[serde(default)]
    pub os: String,
    /// Steps run inside the VM, in order: `.sh` on Linux (and Ansible playbooks, `.yml`),
    /// `.ps1` on Windows. On Linux, the step `reboot` restarts the machine and goes on with the
    /// next step once it is back (a new kernel, a changed boot configuration).
    #[serde(default)]
    pub provision: Vec<String>,
    /// The image to use instead of the built-in one for `os`, per target.
    #[serde(default)]
    pub image: Option<VmImage>,
    /// A virtual TPM 2.0 (BitLocker, Windows Hello, measured boot). Unset, a Windows 11 machine
    /// gets one wherever the target can give one, and runs without where it can't (its
    /// built-in images install without one). `true` requires one: a target that gives none
    /// refuses the environment. `false` gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<bool>,
    /// How it boots, when not the way its image does: its own kernel, firmware, boot order or
    /// network boot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<super::Boot>,
    /// Project scripts run once the machine is up, after its command started (`setup:`). Set by
    /// lowering, never written in a spec.
    #[serde(skip)]
    #[schemars(skip)]
    pub setup: Vec<String>,
}

/// The `vm.provision` step that restarts a Linux machine; the steps after it run once the
/// machine is back.
pub const REBOOT_STEP: &str = "reboot";

/// Whether a `vm.provision` step is the [`REBOOT_STEP`] rather than a file.
pub fn is_reboot(step: &str) -> bool {
    step == REBOOT_STEP
}

/// How a Windows box answers WinRM. Most boxes (StefanScherer, gusztavvargadr) use plain HTTP
/// on 5985 with basic auth; some (GOAD's Windows Server 2025 box) only listen on HTTPS 5986.
/// Isoloom can't tell from the box name, so a non-default box declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Winrm {
    /// HTTP on 5985, basic auth (the default for the common Windows boxes).
    #[default]
    Plaintext,
    /// HTTPS on 5986, NTLM (certificate not verified).
    Ssl,
}

/// How a machine resolves names.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dns {
    /// DNS server addresses, in order (e.g. the lab's domain controller).
    #[serde(default)]
    pub servers: Vec<String>,
    /// Search domains appended to bare names.
    #[serde(default)]
    pub search: Vec<String>,
    /// The machine's own domain name.
    #[serde(default)]
    pub domain: Option<String>,
    /// Resolver options, as resolv.conf's `options` line writes them: `name` or `name:value`
    /// (`ndots:2`, `timeout:1`, `attempts:3`, `rotate`). A container's `dns_opt`, a pod's
    /// `dnsConfig` options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// A machine's own image, per target, instead of the built-in one for its OS.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VmImage {
    /// A Vagrant box name (e.g. `StefanScherer/windows_2019`).
    #[serde(default)]
    pub vagrant: Option<String>,
    /// The box version to pin.
    #[serde(default)]
    pub vagrant_version: Option<String>,
    /// The box Vagrant's QEMU provider boots instead (libvirt format), when `vagrant` has none
    /// (e.g. `peru/windows-server-2019-standard-x64-eval`).
    #[serde(default)]
    pub qemu: Option<String>,
    /// The box Vagrant's Hyper-V provider boots instead, when `vagrant` has no Hyper-V build
    /// (its latest version that has one).
    #[serde(default)]
    pub hyperv: Option<String>,
    /// How this Windows box answers WinRM, when it isn't the usual plain-HTTP box.
    #[serde(default)]
    pub winrm: Option<Winrm>,
    /// This Windows box answers OpenSSH (Windows' own `sshd`, PowerShell as its shell) instead
    /// of WinRM: Vagrant connects with `config.winssh` and the playbooks over SSH, as the box's
    /// `vagrant` account. Not with `winrm`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub winssh: bool,
    /// Where Vagrant downloads the `vagrant` box from, when it isn't on Vagrant Cloud: a URL, or
    /// a `.box` file in the project (list its folder in `.isoloomignore`, or the project copied
    /// into every VM carries it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_url: Option<String>,
    /// The box server Vagrant looks the `vagrant` box up on, instead of Vagrant Cloud (a
    /// private registry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_server_url: Option<String>,
    /// The Proxmox image: a cloud image's URL (the server downloads it), or the id of a template
    /// VM on the node (cloned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxmox: Option<String>,
    /// The AWS image: an AMI id (`ami-0123...`) in the module's region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aws: Option<String>,
    /// The Azure image: a managed image's or gallery image version's resource id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azure: Option<String>,
    /// The Google Cloud image: `projects/<project>/global/images/<image>` (or `.../family/<f>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcp: Option<String>,
    /// The DigitalOcean image: a slug or a custom image's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digitalocean: Option<String>,
    /// The Linode image: `linode/<distribution>` or `private/<id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linode: Option<String>,
    /// The Oracle Cloud image: its OCID (`ocid1.image...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oci: Option<String>,
}

/// The controller's settings (`controller:`), the fields a machine uses for the same things.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Controller {
    /// The box to use instead of Isoloom's (Vagrant): a specific Ansible version, a mirror.
    #[serde(default)]
    #[schemars(skip)] // lowering fills it from `overrides:`; a spec never writes it here.
    pub image: Option<VmImage>,
    /// CPUs, memory (and disk on Proxmox), for many forks against a large environment.
    #[serde(default)]
    pub resources: Option<Resources>,
    /// Keep it running once provisioning is done (to use it as a jump host). By default it
    /// halts, keeping its disk: `isoloom test` and `isoloom provision` boot it when they need it.
    #[serde(default)]
    pub keep_running: bool,
}

/// One environment-level provisioning step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provision {
    /// An Ansible playbook in the project, run against the whole environment.
    pub ansible: String,
    /// More inventory files in the project (groups, variables), next to the one Isoloom writes
    /// (every machine's address and connection, in the `linux` or `windows` group).
    #[serde(default)]
    pub inventory: Vec<String>,
    /// Inventory groups: a name -> the machines in it.
    #[serde(default)]
    pub groups: IndexMap<String, Vec<String>>,
    /// Extra variables for the playbook, of any YAML type (strings, numbers, booleans, lists,
    /// maps): passed as JSON (`-e`), which keeps their types.
    #[serde(default)]
    pub vars: IndexMap<String, serde_json::Value>,
    /// Variables per machine (a machine's name -> its variables), on its line of the inventory
    /// Isoloom writes: what a playbook keys on per host (its domain, a role flag).
    #[serde(default)]
    pub host_vars: IndexMap<String, IndexMap<String, serde_json::Value>>,
    /// The Ansible Galaxy requirements to install first (collections, roles); by default
    /// `requirements.yml` next to the playbook, when there is one.
    #[serde(default)]
    pub requirements: Option<String>,
}

/// The image table's OS names, the ones every target maps to its own images (e.g. Windows from
/// an evaluation ISO locally, a license-included image in the cloud). A spec writes the OS as
/// `<family>@<version>` ([`crate::os`]); a parsed spec's `vm.os` holds one of these names. In
/// the order of [`crate::os::FAMILIES`].
pub const KNOWN_OS: &[&str] = &[
    "debian-11",
    "debian-12",
    "debian-13",
    "ubuntu-14.04",
    "ubuntu-16.04",
    "ubuntu-18.04",
    "ubuntu-20.04",
    "ubuntu-22.04",
    "ubuntu-24.04",
    "rocky-9",
    "almalinux-9",
    "alpine-3.19",
    "kali",
    "fedora-42",
    "centos-7",
    "windows-10",
    "windows-11",
    "windows-server-2008r2",
    "windows-server-2012r2",
    "windows-server-2016",
    "windows-server-2019",
    "windows-server-2022",
    "windows-server-2025",
];
