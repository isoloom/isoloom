//! Set-up that must survive a restart. A step's script (routes, trunks, link impairment, the
//! egress rules) changes kernel state a reboot forgets; a VM backend whose machines restart
//! (a `reboot` step, `vagrant reload`, a stopped cloud VM) wraps it with [`at_boot`] so it is
//! applied now and again at every boot. A machine's own command and `before` steps run the same
//! way, as a `service`.

use isoloom_spec::linux::CONFIG_DIR;
use isoloom_spec::shell::{heredoc, quote};

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
    let path = format!("{CONFIG_DIR}/{name}.sh");
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

/// `script` saved as `/etc/isoloom/<name>.sh` and run as the systemd service `isoloom-<name>`,
/// a process that keeps running (restarted when it fails), started now and at every boot after
/// (and requiring) `after`'s: [`at_boot`]'s script and unit, for a machine's own command.
///
/// Without systemd (OpenRC: Alpine), it runs from `/etc/local.d` at boot, in the background, its
/// output in `/var/log/isoloom-<name>.log`, and now.
#[must_use]
pub(crate) fn service(name: &str, what: &str, script: &str, after: &[&str]) -> String {
    let path = format!("{CONFIG_DIR}/{name}.sh");
    let service = format!("Type=simple\nExecStart={path}\nRestart=on-failure\nRestartSec=2");
    let now = format!("nohup {path} >/var/log/isoloom-{name}.log 2>&1 &");
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
    let (name, what, path) = (u.name, u.what, format!("{CONFIG_DIR}/{}.sh", u.name));
    let unit = format!("isoloom-{name}.service");
    let marker = name.to_uppercase();
    let after: String = u.after.iter().map(|a| format!(" isoloom-{a}.service")).collect::<Vec<_>>().concat();
    let requires = if u.requires && !after.is_empty() {
        format!("\nRequires={}", after.trim_start())
    } else {
        String::new()
    };
    // The script holds the spec's values (an environment value's lines among them): its
    // terminator is no line of it.
    let script = heredoc(&format!("cat > {path}"), &marker, &format!("#!/bin/sh\n# {what}\n{}", u.script));
    let unit_file = heredoc(
        &format!("cat > /etc/systemd/system/{unit}"),
        "UNIT",
        &format!(
            "[Unit]\nDescription={what}\nAfter=network-online.target{after}\nWants=network-online.target{requires}\n\n[Service]\n{}\n\n[Install]\nWantedBy=multi-user.target\n",
            u.service
        ),
    );
    format!(
        "mkdir -p {CONFIG_DIR}\n{script}chmod +x {path}\n{hooks}\
         if command -v systemctl >/dev/null; then\n\
         {unit_file}\
         systemctl daemon-reload\nsystemctl enable {unit}\nsystemctl restart {unit}\n\
         else\n\
         {openrc}rc-update add local default >/dev/null 2>&1 || true\n{now}\n\
         fi\n",
        hooks = u.hooks,
        openrc = u.openrc,
        now = u.now,
    )
}

#[cfg(test)]
mod tests {
    use super::{Trigger, at_boot, service};

    /// The text `command <<'TAG'` writes in `sh`, without its terminator.
    fn heredoc<'a>(sh: &'a str, command: &str) -> &'a str {
        let (_, rest) = sh
            .split_once(&format!("{command} <<'"))
            .unwrap_or_else(|| panic!("no `{command}` here-document in {sh}"));
        let (tag, body) = rest.split_once("'\n").expect("the here-document's tag");
        body.split_once(&format!("\n{tag}\n")).map_or(body, |(text, _)| text)
    }

    /// The `key=value` lines of section `[name]` of the systemd unit `isoloom-<unit>` in `sh`.
    fn section<'a>(sh: &'a str, unit: &str, name: &str) -> Vec<&'a str> {
        let file = heredoc(sh, &format!("cat > /etc/systemd/system/isoloom-{unit}.service"));
        let (_, rest) = file.split_once(&format!("[{name}]\n")).unwrap_or_else(|| panic!("no [{name}] in {file}"));
        rest.lines().take_while(|l| !l.is_empty()).collect()
    }

    /// What `sh` runs without systemd: its `else` branch.
    fn without_systemd(sh: &str) -> &str {
        sh.split_once("\nelse\n")
            .and_then(|(_, rest)| rest.strip_suffix("fi\n"))
            .expect("an else branch")
    }

    #[test]
    fn a_daemon_restarts_and_requires_what_it_runs_after() {
        let sh = service("command", "The machine's command", "exec app\n", &["before"]);
        assert_eq!(heredoc(&sh, "cat > /etc/isoloom/command.sh"), "#!/bin/sh\n# The machine's command\nexec app");
        assert_eq!(
            section(&sh, "command", "Unit"),
            [
                "Description=The machine's command",
                "After=network-online.target isoloom-before.service",
                "Wants=network-online.target",
                "Requires=isoloom-before.service"
            ]
        );
        assert_eq!(
            section(&sh, "command", "Service"),
            ["Type=simple", "ExecStart=/etc/isoloom/command.sh", "Restart=on-failure", "RestartSec=2"]
        );
        assert!(
            without_systemd(&sh).ends_with("\nnohup /etc/isoloom/command.sh >/var/log/isoloom-command.log 2>&1 &\n"),
            "{sh}"
        );
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
        assert_eq!(
            heredoc(&sh, "cat > /etc/isoloom/routes.sh"),
            "#!/bin/sh\n# Routes to the other networks\nip route replace 10.1.0.0/24 via 10.0.0.1\nexit 0",
            "a kept script always succeeds"
        );
        assert_eq!(
            section(&sh, "routes", "Unit"),
            [
                "Description=Routes to the other networks",
                "After=network-online.target isoloom-trunks.service",
                "Wants=network-online.target"
            ],
            "a kept script runs after the others without needing them"
        );
        assert_eq!(
            section(&sh, "routes", "Service"),
            ["Type=oneshot", "RemainAfterExit=yes", "ExecStart=-/etc/isoloom/routes.sh"]
        );
        let (before, _) = sh.split_once("if command -v systemctl").expect("the systemd branch");
        assert!(
            before.ends_with(
                "chmod +x /etc/isoloom/routes.sh\nfor d in /etc/network/if-up.d /etc/networkd-dispatcher/routable.d; do\n  if [ -d \"$d\" ]; then ln -sf /etc/isoloom/routes.sh \"$d/zz-isoloom-routes\"; fi\ndone\n"
            ),
            "it runs again whenever a link comes up: {sh}"
        );
        let systemd = sh
            .split_once("if command -v systemctl >/dev/null; then\n")
            .and_then(|(_, r)| r.split_once("\nelse\n"));
        assert!(
            systemd
                .is_some_and(|(branch, _)| branch
                    .ends_with("\nsystemctl daemon-reload\nsystemctl enable isoloom-routes.service\nsystemctl restart isoloom-routes.service")),
            "enabled, so it runs at every boot: {sh}"
        );
        let tc = at_boot("tc", "Link impairment", "tc qdisc\n", Trigger::Boot, &[]);
        let (written, _) = tc.split_once("if command -v systemctl").expect("the systemd branch");
        assert!(written.ends_with("chmod +x /etc/isoloom/tc.sh\n"), "at boot alone, no link hook: {tc}");
        assert_eq!(
            without_systemd(&tc),
            "mkdir -p /etc/local.d && ln -sf /etc/isoloom/tc.sh /etc/local.d/isoloom-tc.start\nrc-update add local default >/dev/null 2>&1 || true\n/etc/isoloom/tc.sh || true\n"
        );
    }

    #[test]
    fn it_runs_under_sh() {
        let Some(mut posix) = crate::test_support::posix_sh() else { return };
        let scratch = tempfile::Builder::new().prefix("isoloom-boot-").tempdir().expect("a temporary folder");
        let at = crate::test_support::sh_path(&scratch.path().join("etc"));
        // Only the file part: a test can't enable a systemd unit.
        let sh = at_boot("t", "test", "echo ok\n", Trigger::Boot, &[]).replace("/etc/isoloom", &at);
        let file_part = sh.split("if command -v systemctl").next().unwrap_or_default().to_owned();
        let out = posix.arg("-c").arg(format!("{file_part}{at}/t.sh")).output().expect("sh runs");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "ok\n");
        assert!(out.status.success());
    }
}
