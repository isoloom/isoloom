//! A machine's set-up as a cloud-init document (`#cloud-config`), for a target that hands each
//! VM its user data (Proxmox): the constraints' `Prelude` step as `bootcmd` (before packages
//! install, at every boot: it is idempotent), the files as `write_files`, the other steps as
//! `runcmd`, run as root. After a `reboot` step, cloud-init restarts the machine once it is done
//! (`power_state`) and a unit runs the remaining steps at the next boot. The data disks are
//! cloud-init's own: each found by its serial (the disk's name, as the target attaches it),
//! partitioned, formatted with its name as the label and mounted from `/etc/fstab`.

use isoloom_plan::{DataDisk, Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::{Expression, boolean, call, list, object, string};
use isoloom_render::steps;

use isoloom_spec::shell::quote;

use super::{Shell, at_every_boot};
use crate::module::inputs;

/// Where a machine's set-up says how far it got, in the `isoloom-ready/1` grammar: `running:
/// <step>` while a step runs, `failed: <step>` once one failed (nothing after it runs), `ready`
/// once it is done. No file yet: still booting.
pub(crate) const READY_FILE: &str = "/var/lib/isoloom/ready";

/// Where a machine with `reboot` steps keeps its continuation: `next-boot` (the part to run at the
/// next boot), `steps.sh` and `boot-<n>.sh`.
const REBOOT_DIR: &str = "/var/lib/isoloom";

/// A cloud-config document, built up.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CloudConfig {
    /// The machine's host name.
    pub(crate) hostname: String,
    /// Its full name (`<hostname>.<domain>`), when it has a domain: the hosts file's entry for
    /// it, the host name staying the short one.
    pub(crate) fqdn: Option<String>,
    /// Its users (an expression: the module's `local.users`).
    pub(crate) users: Expression,
    /// Packages cloud-init installs.
    pub(crate) packages: Vec<String>,
    /// Commands at every boot, before anything else.
    pub(crate) bootcmd: Vec<String>,
    /// Lists of files (an expression each: the project's).
    pub(crate) file_lists: Vec<Expression>,
    /// Files, each an object (see [`file()`]).
    pub(crate) files: Vec<Expression>,
    /// Commands at the first boot, in order.
    pub(crate) runcmd: Vec<String>,
    /// Whether cloud-init restarts the machine once done (a `reboot` step left a part).
    pub(crate) restart: bool,
    /// The data disks it formats and mounts (raw ones are left as they are).
    pub(crate) disks: Vec<DataDisk>,
    /// Whether a command says its progress in [`READY_FILE`] (its folder is made first).
    tracked: bool,
}

impl CloudConfig {
    /// An empty document for `hostname`.
    pub(crate) fn new(hostname: &str, users: Expression) -> CloudConfig {
        CloudConfig {
            hostname: hostname.to_owned(),
            fqdn: None,
            users,
            packages: Vec::new(),
            bootcmd: Vec::new(),
            file_lists: Vec::new(),
            files: Vec::new(),
            runcmd: Vec::new(),
            restart: false,
            disks: Vec::new(),
            tracked: false,
        }
    }

    /// Adds `command` to `runcmd` as the step `label` (see [`tracked`]).
    pub(crate) fn track(&mut self, label: &str, command: &str) {
        self.ready_dir();
        self.runcmd.push(tracked(label, command));
    }

    /// Makes the ready file's folder first, once.
    fn ready_dir(&mut self) {
        if !self.tracked {
            self.runcmd.push(format!("mkdir -p {READY_DIR}"));
            self.tracked = true;
        }
    }

    /// The document: `#cloud-config` then the YAML Terraform encodes.
    pub(crate) fn document(self) -> Expression {
        let mut items = vec![("hostname", string(self.hostname))];
        items.extend(self.fqdn.map(|f| ("fqdn", string(f))));
        items.extend([("users", self.users), ("packages", list(self.packages.into_iter().map(string)))]);
        if !self.bootcmd.is_empty() {
            items.push(("bootcmd", list(self.bootcmd.into_iter().map(string))));
        }
        items.extend(disks(&self.disks));
        items.push(("write_files", write_files(self.file_lists, self.files)));
        items.push(("runcmd", list(self.runcmd.into_iter().map(string))));
        if self.restart {
            let power = object([
                ("mode", string("reboot")),
                ("message", string("isoloom: the reboot step")),
                ("condition", string(format!("test -f {REBOOT_DIR}/next-boot"))),
            ]);
            items.push(("power_state", power));
        }
        call("format", [string("#cloud-config\n%s"), call("yamlencode", [object(items)])])
    }
}

/// The ready file's folder.
const READY_DIR: &str = "/var/lib/isoloom";

/// `command` as the set-up step `label`: its name in [`READY_FILE`] first; when it fails,
/// `failed: <label>` there and the end of the set-up (cloud-init runs `runcmd` as one script
/// that otherwise goes on to the next command, and to `ready`).
pub(crate) fn tracked(label: &str, command: &str) -> String {
    let running = quote(&format!("running: {label}"));
    let failed = quote(&format!("failed: {label}"));
    format!("echo {running} > {READY_FILE} && {command} || {{ echo {failed} > {READY_FILE}; exit 1; }}")
}

/// What the ready file calls a step of `machine`'s set-up.
fn label(step: &Step) -> String {
    match step {
        Step::Run { path, .. } => path.clone(),
        Step::WaitFor { machine, .. } => format!("waiting for {machine}"),
        Step::SharedVolume { volume, .. } => format!("shared volume {volume}"),
        Step::DiskGrow => "growing the disk".into(),
        Step::Disks { .. } => "data disks".into(),
        Step::Hosts { .. } => "hosts file".into(),
        Step::Settings => "system settings".into(),
        Step::Trunks => "VLAN interfaces".into(),
        Step::MoreAddresses { .. } => "more addresses".into(),
        Step::Routes { .. } => "routes".into(),
        Step::Ipv6 { .. } => "IPv6".into(),
        Step::Volumes { .. } => "volume folders".into(),
        Step::Files { .. } => "files".into(),
        Step::Secrets => "waiting for its secrets".into(),
        Step::Inputs { .. } => "inputs".into(),
        Step::Before { .. } => "before scripts".into(),
        Step::Command => "starting its command".into(),
        Step::Setup { .. } => "setup scripts".into(),
        Step::Egress => "closing the internet".into(),
        Step::Tc => "link impairment".into(),
        Step::Ready => "health check".into(),
        _ => "set-up".into(),
    }
}

/// The prefix of a disk's device, whose serial is its name: what a virtio disk is under udev.
const DISK_BY_ID: &str = "/dev/disk/by-id/virtio-";

/// The keys formatting and mounting the formatted disks: each one's alias (its device, by its
/// serial), one partition on a GPT table, its filesystem labelled with its name, its fstab line.
/// Nothing on a disk that has a partition table or a filesystem already (`overwrite: false`).
fn disks(disks: &[DataDisk]) -> Vec<(&'static str, Expression)> {
    let formatted: Vec<(&DataDisk, &str)> = disks.iter().filter_map(|d| Some((d, d.mount.as_deref()?))).collect();
    if formatted.is_empty() {
        return Vec::new();
    }
    let aliases = formatted.iter().map(|(d, _)| (d.name.as_str(), string(format!("{DISK_BY_ID}{}", d.name))));
    let setup = formatted.iter().map(|(d, _)| {
        let table = object([("table_type", string("gpt")), ("layout", boolean(true)), ("overwrite", boolean(false))]);
        (d.name.as_str(), table)
    });
    let fs = formatted.iter().map(|(d, _)| {
        object([
            ("label", string(d.name.as_str())),
            ("filesystem", string(d.fs.id())),
            ("device", string(format!("{}.1", d.name))),
        ])
    });
    let mounts = formatted.iter().map(|(d, at)| {
        list(
            [
                format!("LABEL={}", d.name),
                (*at).to_owned(),
                d.fs.id().to_owned(),
                "defaults,nofail".into(),
                "0".into(),
                "2".into(),
            ]
            .map(string),
        )
    });
    vec![
        ("device_aliases", object(aliases)),
        ("disk_setup", object(setup)),
        ("fs_setup", list(fs)),
        ("mounts", list(mounts)),
    ]
}

/// The files: the lists, then the single files.
fn write_files(lists: Vec<Expression>, files: Vec<Expression>) -> Expression {
    if lists.is_empty() {
        return list(files);
    }
    let mut args = lists;
    if !files.is_empty() {
        args.push(list(files));
    }
    if args.len() == 1 { args.remove(0) } else { call("concat", args) }
}

/// A `write_files` entry.
pub(crate) fn file(path: &str, permissions: Option<&str>, content: Expression) -> Expression {
    let mut items = vec![("path", string(path))];
    items.extend(permissions.map(|p| ("permissions", string(p))));
    items.push(("content", content));
    object(items)
}

/// The constraints' preamble as a `bootcmd` command, when the spec has constraints.
pub(crate) fn prelude(env: &Environment) -> Option<String> {
    Shell::Root.step(env, "", &Step::Prelude)
}

/// Adds `machine`'s steps to `config`: the prelude, the inputs file, the steps (the project is
/// the target's: its files are written in place), and what a `reboot` step needs.
pub(crate) fn machine(env: &Environment, (machine, mp): (&str, &MachinePlan), config: &mut CloudConfig) {
    config.bootcmd.extend(prelude(env));
    if !mp.inputs.is_empty() {
        // Written outside /tmp (emptied at boot, maybe after cloud-init writes its files), then
        // put where the steps read it once the boot is done.
        config.files.push(file(steps::KEPT_INPUTS_FILE, Some("0600"), inputs::file(&mp.inputs)));
        config.track("inputs", &format!("install -m 600 {} {}", steps::KEPT_INPUTS_FILE, steps::INPUTS_FILE));
    }
    let mut parts: Vec<Vec<String>> = vec![Vec::new()];
    let mut again: Vec<String> = Vec::new();
    for step in &mp.lifecycle {
        match step {
            Step::Prelude | Step::Project => {}
            Step::Disks { disks } => config.disks.clone_from(disks),
            Step::Reboot => parts.push(again.clone()),
            _ => {
                let Some(c) = Shell::Root.step(env, machine, step).map(|c| tracked(&label(step), &c)) else { continue };
                if at_every_boot(step) {
                    again.push(c.clone());
                }
                parts.last_mut().into_iter().for_each(|p| p.push(c.clone()));
            }
        }
    }
    let mut parts = parts.into_iter();
    let first = parts.next().unwrap_or_default();
    if !first.is_empty() {
        config.ready_dir();
    }
    config.runcmd.extend(first);
    let later: Vec<Vec<String>> = parts.collect();
    if !later.is_empty() {
        config.runcmd.push(format!(
            "mkdir -p {REBOOT_DIR} && echo 1 > {REBOOT_DIR}/next-boot && systemctl enable isoloom-steps.service"
        ));
        config.files.extend(reboot_files(&later));
        config.restart = true;
    }
}

/// The files of a machine's `reboot` steps: a unit run at each boot while `next-boot` names a
/// part, the script it runs, and each part after a reboot (`boot-1.sh` after the first): its
/// commands, then either the next reboot or the unit's end. A part stops at its first failing
/// command: no ready marker then (`journalctl -u isoloom-steps` says why).
fn reboot_files(parts: &[Vec<String>]) -> Vec<Expression> {
    let unit = "[Unit]\nDescription=Isoloom: the provisioning steps after a reboot\nWants=network-online.target\nAfter=network-online.target\nConditionPathExists=/var/lib/isoloom/next-boot\n\n[Service]\nType=oneshot\nExecStart=/bin/sh /var/lib/isoloom/steps.sh\nStandardOutput=journal+console\nTimeoutStartSec=0\n\n[Install]\nWantedBy=multi-user.target\n";
    let steps = format!("#!/bin/sh\nset -e\nn=$(cat {REBOOT_DIR}/next-boot)\nrm -f {REBOOT_DIR}/next-boot\nexec sh -e {REBOOT_DIR}/boot-$n.sh\n");
    let mut files = vec![
        file("/etc/systemd/system/isoloom-steps.service", Some("0644"), string(unit)),
        file(&format!("{REBOOT_DIR}/steps.sh"), Some("0700"), string(steps)),
    ];
    for (n, cmds) in (1..).zip(parts) {
        let end = if n < parts.len() {
            format!("echo {} > {REBOOT_DIR}/next-boot\nsystemctl --no-block reboot\n", n + 1)
        } else {
            "systemctl disable isoloom-steps.service\n".to_owned()
        };
        let script = format!("#!/bin/sh\nset -e\n{}{end}", cmds.iter().map(|c| format!("{c}\n")).collect::<Vec<_>>().concat());
        files.push(file(&format!("{REBOOT_DIR}/boot-{n}.sh"), Some("0700"), string(script)));
    }
    files
}
