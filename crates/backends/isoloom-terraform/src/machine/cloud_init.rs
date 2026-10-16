//! A machine's set-up as a cloud-init document (`#cloud-config`), for a target that hands each
//! VM its user data (Proxmox): the constraints' `Prelude` step as `bootcmd` (before packages
//! install, at every boot: it is idempotent), the files as `write_files`, the other steps as
//! `runcmd`, run as root. After a `reboot` step, cloud-init restarts the machine once it is done
//! (`power_state`) and a unit runs the remaining steps at the next boot. The data disks are
//! cloud-init's own: each found by its serial (the disk's name, as the target attaches it),
//! partitioned, formatted with its name as the label and mounted from `/etc/fstab`.

use isoloom_plan::configure::Resource;
use isoloom_plan::{DataDisk, Environment, MachinePlan, Step};
use isoloom_render::configure::Configured;
use isoloom_render::configure::cloud_init as cloud_init_carrier;
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

    /// The document: `#cloud-config` then the YAML Terraform encodes. A list with nothing in it
    /// is left out: cloud-init's schema wants its lists non-empty.
    pub(crate) fn document(self) -> Expression {
        let mut items = vec![("hostname", string(self.hostname))];
        items.extend(self.fqdn.map(|f| ("fqdn", string(f))));
        items.push(("users", self.users));
        items.extend(commands("packages", self.packages));
        items.extend(commands("bootcmd", self.bootcmd));
        items.extend(disks(&self.disks));
        items.extend(write_files(self.file_lists, self.files).map(|f| ("write_files", f)));
        items.extend(commands("runcmd", self.runcmd));
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
/// that otherwise goes on to the next command, and to `ready`). The progress mark is
/// best-effort, its own command rather than chained with `&&` (`A && B || C` reads as an
/// if-then-else that isn't one): a mark that can't be written doesn't skip the step, and the
/// step's own failure always reaches the failure mark.
pub(crate) fn tracked(label: &str, command: &str) -> String {
    let running = quote(&format!("running: {label}"));
    let failed = quote(&format!("failed: {label}"));
    format!("echo {running} > {READY_FILE}; {command} || {{ echo {failed} > {READY_FILE}; exit 1; }}")
}

/// What the ready file calls a step of `machine`'s set-up.
fn label(step: &Step) -> String {
    match step {
        Step::Run { path, .. } => path.clone(),
        Step::Configure { .. } => "its configuration".into(),
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

/// `key` and its list of words (packages or commands), unless it has none.
fn commands(key: &'static str, words: Vec<String>) -> Option<(&'static str, Expression)> {
    (!words.is_empty()).then(|| (key, list(words.into_iter().map(string))))
}

/// The files: the lists, then the single files. Nothing when there are neither.
fn write_files(lists: Vec<Expression>, files: Vec<Expression>) -> Option<Expression> {
    if lists.is_empty() {
        return (!files.is_empty()).then(|| list(files));
    }
    let mut args = lists;
    if !files.is_empty() {
        args.push(list(files));
    }
    Some(if args.len() == 1 { args.remove(0) } else { call("concat", args) })
}

/// A `write_files` entry.
pub(crate) fn file(path: &str, permissions: Option<&str>, content: Expression) -> Expression {
    let mut items = vec![("path", string(path))];
    items.extend(permissions.map(|p| ("permissions", string(p))));
    items.push(("content", content));
    object(items)
}

/// A `write_files` entry whose content cloud-init decodes first (`encoding`, such as `gz+b64`):
/// a recipe's configure script, compressed (see `isoloom_render::configure::cloud_init`).
pub(crate) fn encoded_file(path: &str, encoding: &str, permissions: &str, content: &str) -> Expression {
    object([
        ("path", string(path)),
        ("encoding", string(encoding)),
        ("permissions", string(permissions)),
        ("content", string(content)),
    ])
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
    let mut configures = 0usize;
    for step in &mp.lifecycle {
        match step {
            Step::Prelude | Step::Project => {}
            Step::Disks { disks } => config.disks.clone_from(disks),
            Step::Reboot => parts.push(again.clone()),
            // The recipe's script is carried in `write_files`, compressed, and run from here, so
            // the library isn't inlined in every machine's `runcmd`: the same text a VM's shell
            // set-up runs (`isoloom_render::configure::cloud_init`).
            Step::Configure { resources, fresh } => {
                if let Some((cmd, written)) = configure_run(env, machine, resources, *fresh, configures) {
                    configures += 1;
                    config.files.push(written);
                    parts.last_mut().into_iter().for_each(|p| p.push(cmd.clone()));
                }
            }
            _ => {
                let Some(c) = Shell::Root.step(env, machine, step).map(|c| tracked(&label(step), &c)) else {
                    continue;
                };
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

/// Machine `machine`'s `index`-th configure run: the `runcmd` command that runs it (tracked in
/// the ready file), and the `write_files` entry that delivers its script compressed. The script
/// is the shell renderer's own, so cloud-init leaves what the shell set-up leaves. `None` only
/// when the machine has no recipe (nothing to configure).
fn configure_run(env: &Environment, machine: &str, resources: &[Resource], fresh: bool, index: usize) -> Option<(String, Expression)> {
    let c = Configured::of(env, machine)?;
    let carried = cloud_init_carrier::configure(&c, resources, fresh, index);
    let written = encoded_file(&carried.file.path, "gz+b64", carried.file.permissions, &carried.file.content);
    Some((tracked(cloud_init_carrier::LABEL, &carried.command), written))
}

/// The files of a machine's `reboot` steps: a unit run at each boot while `next-boot` names a
/// part, the script it runs, and each part after a reboot (`boot-1.sh` after the first): its
/// commands, then either the next reboot or the unit's end. A part stops at its first failing
/// command: no ready marker then (`journalctl -u isoloom-steps` says why).
fn reboot_files(parts: &[Vec<String>]) -> Vec<Expression> {
    let unit = "[Unit]\nDescription=Isoloom: the provisioning steps after a reboot\nWants=network-online.target\nAfter=network-online.target\nConditionPathExists=/var/lib/isoloom/next-boot\n\n[Service]\nType=oneshot\nExecStart=/bin/sh /var/lib/isoloom/steps.sh\nStandardOutput=journal+console\nTimeoutStartSec=0\n\n[Install]\nWantedBy=multi-user.target\n";
    let steps = format!("#!/bin/sh\nset -e\nn=$(cat {REBOOT_DIR}/next-boot)\nrm -f {REBOOT_DIR}/next-boot\nexec sh -e \"{REBOOT_DIR}/boot-$n.sh\"\n");
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
