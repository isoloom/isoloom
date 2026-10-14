//! A machine's set-up as a cloud-init document (`#cloud-config`), for a target that hands each
//! VM its user data (Proxmox): the constraints' `Prelude` step as `bootcmd` (before packages
//! install, at every boot: it is idempotent), the files as `write_files`, the other steps as
//! `runcmd`, run as root. After a `reboot` step, cloud-init restarts the machine once it is done
//! (`power_state`) and a unit runs the remaining steps at the next boot.

use std::fmt::Write as _;

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Expression, call, list, object, string};
use isoloom_render::steps;

use super::{at_every_boot, module, step_command};

/// Where a machine with `reboot` steps keeps its continuation: `next-boot` (the part to run at the
/// next boot), `steps.sh` and `boot-<n>.sh`.
const REBOOT_DIR: &str = "/var/lib/isoloom";

/// A cloud-config document, built up.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudConfig {
    /// The machine's host name.
    pub hostname: String,
    /// Its full name (`<hostname>.<domain>`), when it has a domain: the hosts file's entry for
    /// it, the host name staying the short one.
    pub fqdn: Option<String>,
    /// Its users (an expression: the module's `local.users`).
    pub users: Expression,
    /// Packages cloud-init installs.
    pub packages: Vec<String>,
    /// Commands at every boot, before anything else.
    pub bootcmd: Vec<String>,
    /// Lists of files (an expression each: the project's).
    pub file_lists: Vec<Expression>,
    /// Files, each an object (see [`file()`]).
    pub files: Vec<Expression>,
    /// Commands at the first boot, in order.
    pub runcmd: Vec<String>,
    /// Whether cloud-init restarts the machine once done (a `reboot` step left a part).
    pub restart: bool,
}

impl CloudConfig {
    /// An empty document for `hostname`.
    pub fn new(hostname: &str, users: Expression) -> CloudConfig {
        CloudConfig {
            hostname: hostname.to_string(),
            fqdn: None,
            users,
            packages: Vec::new(),
            bootcmd: Vec::new(),
            file_lists: Vec::new(),
            files: Vec::new(),
            runcmd: Vec::new(),
            restart: false,
        }
    }

    /// The document: `#cloud-config` then the YAML Terraform encodes.
    pub fn document(self) -> Expression {
        let mut items = vec![("hostname", string(self.hostname))];
        items.extend(self.fqdn.map(|f| ("fqdn", string(f))));
        items.extend([("users", self.users), ("packages", list(self.packages.into_iter().map(string)))]);
        if !self.bootcmd.is_empty() {
            items.push(("bootcmd", list(self.bootcmd.into_iter().map(string))));
        }
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
pub fn file(path: &str, permissions: Option<&str>, content: Expression) -> Expression {
    let mut items = vec![("path", string(path))];
    items.extend(permissions.map(|p| ("permissions", string(p))));
    items.push(("content", content));
    object(items)
}

/// The constraints' preamble as a `bootcmd` command, when the spec has constraints.
pub fn prelude(env: &Environment) -> Option<String> {
    step_command(env, "", &Step::Prelude, false)
}

/// Adds `machine`'s steps to `config`: the prelude, the inputs file, the steps (the project is
/// the target's: its files are written in place), and what a `reboot` step needs.
pub fn machine(env: &Environment, machine: &str, config: &mut CloudConfig) {
    let mp = &env.machines[machine];
    config.bootcmd.extend(prelude(env));
    if !mp.inputs.is_empty() {
        // Written outside /tmp (emptied at boot, maybe after cloud-init writes its files), then
        // put where the steps read it once the boot is done.
        config.files.push(file(steps::KEPT_INPUTS_FILE, Some("0600"), module::inputs_file(&mp.inputs)));
        config.runcmd.push(format!("install -m 600 {} {}", steps::KEPT_INPUTS_FILE, steps::INPUTS_FILE));
    }
    let mut parts: Vec<Vec<String>> = vec![Vec::new()];
    let mut again: Vec<String> = Vec::new();
    for step in &mp.lifecycle {
        match step {
            Step::Prelude | Step::Project => {}
            Step::Reboot => parts.push(again.clone()),
            _ => {
                let Some(c) = step_command(env, machine, step, false) else { continue };
                if at_every_boot(step) {
                    again.push(c.clone());
                }
                parts.last_mut().into_iter().for_each(|p| p.push(c.clone()));
            }
        }
    }
    let mut parts = parts.into_iter();
    config.runcmd.extend(parts.next().unwrap_or_default());
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
    for (i, cmds) in parts.iter().enumerate() {
        let n = i + 1;
        let mut script = String::from("#!/bin/sh\nset -e\n");
        for c in cmds {
            script.push_str(c);
            script.push('\n');
        }
        if n < parts.len() {
            let _ = writeln!(script, "echo {} > {REBOOT_DIR}/next-boot\nsystemctl --no-block reboot", n + 1);
        } else {
            script.push_str("systemctl disable isoloom-steps.service\n");
        }
        files.push(file(&format!("{REBOOT_DIR}/boot-{n}.sh"), Some("0700"), string(script)));
    }
    files
}
