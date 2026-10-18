//! Set-up that must survive a restart. A step's script (routes, trunks, link impairment, the
//! egress rules) changes kernel state a reboot forgets; a VM backend whose machines restart
//! (a `reboot` step, `vagrant reload`, a stopped cloud VM) wraps it with [`at_boot`] so it is
//! applied now and again at every boot. A machine's own command and `before` steps run the same
//! way, as a [`service`].

use isoloom_spec::shell::quote;

/// When a script kept with [`at_boot`] runs again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// At every boot.
    Boot,
    /// At every boot, and whenever an interface comes up (ifupdown, networkd-dispatcher):
    /// bringing an interface down drops its routes, and some hypervisors (Vagrant) reconfigure
    /// interfaces after every boot.
    BootAndLinkUp,
}

/// `script` saved as `/etc/isoloom/<name>.sh` and run now and again as `trigger` says, by a
/// systemd service (`isoloom-<name>`), `what` saying what it does.
///
/// `after`: the other such scripts it runs after at boot (routes through a trunk's interface
/// after the trunks).
///
/// Without systemd (OpenRC: Alpine), it runs from `/etc/local.d` at boot instead.
///
/// The script always succeeds (a route whose link isn't up yet, a VLAN interface already
/// there): an interface hook failing would fail the interface's bring-up. It runs again at the
/// next boot or link change.
#[must_use]
pub fn at_boot(name: &str, what: &str, script: &str, trigger: Trigger, after: &[&str]) -> String {
    let path = format!("/etc/isoloom/{name}.sh");
    let hooks = match trigger {
        Trigger::BootAndLinkUp => format!(
            "for d in /etc/network/if-up.d /etc/networkd-dispatcher/routable.d; do\n  if [ -d \"$d\" ]; then ln -sf {path} \"$d/zz-isoloom-{name}\"; fi\ndone\n"
        ),
        Trigger::Boot => String::new(),
    };
    install(&Unit {
        name,
        what,
        script: &format!("{script}exit 0\n"),
        hooks: &hooks,
        after,
        requires: false,
        service: &format!("Type=oneshot\nRemainAfterExit=yes\nExecStart=-{path}"),
        openrc: &format!("mkdir -p /etc/local.d && ln -sf {path} /etc/local.d/isoloom-{name}.start\n"),
        now: &format!("{path} || true"),
    })
}

/// How a [`service`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Runs to its end, now and at every boot; a failure fails the step that installs it
    /// (a machine's `before` scripts).
    Once,
    /// Keeps running, restarted when it fails (a machine's own command).
    Daemon,
}

/// `script` saved as `/etc/isoloom/<name>.sh` and run as the systemd service `isoloom-<name>`,
/// started now and at every boot after (and requiring) `after`'s: [`at_boot`]'s script and
/// unit, for a step whose failure matters (`Kind::Once`) or a process that keeps running
/// (`Kind::Daemon`).
///
/// Without systemd (OpenRC: Alpine), it runs from `/etc/local.d` at boot (in the background
/// for a daemon, its output in `/var/log/isoloom-<name>.log`), and now.
#[must_use]
pub fn service(name: &str, what: &str, script: &str, kind: Kind, after: &[&str]) -> String {
    let path = format!("/etc/isoloom/{name}.sh");
    let (service, now) = match kind {
        Kind::Once => (format!("Type=oneshot\nRemainAfterExit=yes\nExecStart={path}"), path.clone()),
        Kind::Daemon => (
            format!("Type=simple\nExecStart={path}\nRestart=on-failure\nRestartSec=2"),
            format!("nohup {path} >/var/log/isoloom-{name}.log 2>&1 &"),
        ),
    };
    let start = format!("/etc/local.d/isoloom-{name}.start");
    install(&Unit {
        name,
        what,
        script,
        hooks: "",
        after,
        requires: true,
        service: &service,
        openrc: &format!(
            "mkdir -p /etc/local.d && printf '%s\\n' '#!/bin/sh' {} > {start} && chmod +x {start}\n",
            quote(&now)
        ),
        now: &now,
    })
}

/// A script kept on the machine and the service running it (see [`at_boot`] and [`service`]).
struct Unit<'a> {
    name: &'a str,
    what: &'a str,
    /// The script's lines, after its `#!/bin/sh` and its description.
    script: &'a str,
    /// Run once the script is written (interface hooks).
    hooks: &'a str,
    /// The other such services it starts after.
    after: &'a [&'a str],
    /// Whether it also requires them (it can't run without them).
    requires: bool,
    /// The unit's `[Service]` section.
    service: &'a str,
    /// How OpenRC runs it at boot.
    openrc: &'a str,
    /// How it runs now, without systemd.
    now: &'a str,
}

/// The script written, then its systemd unit enabled and started, or OpenRC's way without
/// systemd.
fn install(u: &Unit) -> String {
    let (name, what, path) = (u.name, u.what, format!("/etc/isoloom/{}.sh", u.name));
    let unit = format!("isoloom-{name}.service");
    let marker = name.to_uppercase();
    let after: String = u.after.iter().map(|a| format!(" isoloom-{a}.service")).collect::<Vec<_>>().concat();
    let requires = if u.requires && !after.is_empty() {
        format!("\nRequires={}", after.trim_start())
    } else {
        String::new()
    };
    format!(
        "mkdir -p /etc/isoloom\ncat > {path} <<'{marker}'\n#!/bin/sh\n# {what}\n{script}{marker}\nchmod +x {path}\n{hooks}\
         if command -v systemctl >/dev/null; then\n\
         cat > /etc/systemd/system/{unit} <<'UNIT'\n[Unit]\nDescription={what}\nAfter=network-online.target{after}\nWants=network-online.target{requires}\n\n\
         [Service]\n{service}\n\n[Install]\nWantedBy=multi-user.target\nUNIT\n\
         systemctl daemon-reload\nsystemctl enable {unit}\nsystemctl restart {unit}\n\
         else\n\
         {openrc}rc-update add local default >/dev/null 2>&1 || true\n{now}\n\
         fi\n",
        script = u.script,
        hooks = u.hooks,
        service = u.service,
        openrc = u.openrc,
        now = u.now,
    )
}

#[cfg(test)]
mod tests {
    use super::{Kind, Trigger, at_boot, service};

    #[test]
    fn a_daemon_restarts_and_a_step_fails_its_setup() {
        let sh = service("command", "The machine's command", "exec app\n", Kind::Daemon, &["before"]);
        assert!(
            sh.contains("cat > /etc/isoloom/command.sh <<'COMMAND'\n#!/bin/sh\n# The machine's command\nexec app\nCOMMAND\n"),
            "{sh}"
        );
        assert!(sh.contains("Type=simple\nExecStart=/etc/isoloom/command.sh\nRestart=on-failure"), "{sh}");
        assert!(
            sh.contains("After=network-online.target isoloom-before.service\nWants=network-online.target\nRequires=isoloom-before.service\n"),
            "{sh}"
        );
        assert!(sh.contains("nohup /etc/isoloom/command.sh >/var/log/isoloom-command.log 2>&1 &"), "{sh}");
        let once = service("before", "Before", "sh x.sh\n", Kind::Once, &[]);
        assert!(once.contains("Type=oneshot\nRemainAfterExit=yes\nExecStart=/etc/isoloom/before.sh\n"), "{once}");
        assert!(!once.contains("ExecStart=-") && !once.contains("Requires="), "a failure is reported: {once}");
    }

    #[test]
    fn the_script_is_kept_and_run_at_every_boot() {
        let sh = at_boot(
            "routes",
            "Routes to the other networks",
            "ip route replace 10.1.0.0/24 via 10.0.0.1\n",
            Trigger::BootAndLinkUp,
            &["trunks"],
        );
        assert!(
            sh.contains("cat > /etc/isoloom/routes.sh <<'ROUTES'\n#!/bin/sh\n# Routes to the other networks\nip route replace"),
            "{sh}"
        );
        assert!(
            sh.contains("ExecStart=-/etc/isoloom/routes.sh") && sh.contains("systemctl enable isoloom-routes.service"),
            "{sh}"
        );
        assert!(
            sh.contains("zz-isoloom-routes") && sh.contains("After=network-online.target isoloom-trunks.service\n"),
            "{sh}"
        );
        assert!(!sh.contains("Requires="), "a kept script runs after the others without needing them: {sh}");
        let tc = at_boot("tc", "Link impairment", "tc qdisc\n", Trigger::Boot, &[]);
        assert!(!tc.contains("if-up.d") && !tc.contains("Requires="), "{tc}");
        assert!(tc.contains("tc qdisc\nexit 0\nTC\n"), "a kept script always succeeds: {tc}");
    }

    #[test]
    fn it_runs_under_sh() {
        let dir = std::env::temp_dir().join(format!("isoloom-boot-{}", std::process::id()));
        // Only the file part: a test can't enable a systemd unit.
        let sh = at_boot("t", "test", "echo ok\n", Trigger::Boot, &[]).replace("/etc/isoloom", &dir.display().to_string());
        let file_part = sh.split("if command -v systemctl").next().unwrap_or_default().to_owned();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{file_part}{}/t.sh", dir.display()))
            .output();
        std::fs::remove_dir_all(&dir).ok();
        let out = out.expect("sh runs");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "ok\n");
        assert!(out.status.success());
    }
}
