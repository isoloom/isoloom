//! Set-up that must survive a restart. A step's script (routes, trunks, link impairment, the
//! egress rules) changes kernel state a reboot forgets; a VM backend whose machines restart
//! (a `reboot` step, `vagrant reload`, a stopped cloud VM) wraps it with [`at_boot`] so it is
//! applied now and again at every boot.

use std::fmt::Write as _;

/// `script` saved as `/etc/isoloom/<name>.sh` and run now and at every boot by a systemd
/// service (`isoloom-<name>`), `what` saying what it does. With `on_link_up`, it also runs
/// whenever an interface comes up (ifupdown, networkd-dispatcher): bringing an interface down
/// drops its routes, and some hypervisors (Vagrant) reconfigure interfaces after every boot.
///
/// `after`: the other such scripts it runs after at boot (routes through a trunk's interface
/// after the trunks).
///
/// Without systemd (OpenRC: Alpine), it runs from `/etc/local.d` at boot instead.
///
/// The script always succeeds (a route whose link isn't up yet, a VLAN interface already
/// there): an interface hook failing would fail the interface's bring-up. It runs again at the
/// next boot or link change.
pub fn at_boot(name: &str, what: &str, script: &str, on_link_up: bool, after: &[&str]) -> String {
    let path = format!("/etc/isoloom/{name}.sh");
    let unit = format!("isoloom-{name}.service");
    let marker = name.to_uppercase();
    let hooks = if on_link_up {
        format!(
            "for d in /etc/network/if-up.d /etc/networkd-dispatcher/routable.d; do\n  if [ -d \"$d\" ]; then ln -sf {path} \"$d/zz-isoloom-{name}\"; fi\ndone\n"
        )
    } else {
        String::new()
    };
    let after = after.iter().fold(String::new(), |mut s, a| {
        let _ = write!(s, " isoloom-{a}.service");
        s
    });
    format!(
        "mkdir -p /etc/isoloom\ncat > {path} <<'{marker}'\n#!/bin/sh\n# {what}\n{script}exit 0\n{marker}\nchmod +x {path}\n{hooks}\
         if command -v systemctl >/dev/null; then\n\
         cat > /etc/systemd/system/{unit} <<'UNIT'\n[Unit]\nDescription={what}\nAfter=network-online.target{after}\nWants=network-online.target\n\n\
         [Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=-{path}\n\n[Install]\nWantedBy=multi-user.target\nUNIT\n\
         systemctl daemon-reload\nsystemctl enable {unit}\nsystemctl restart {unit}\n\
         else\n\
         mkdir -p /etc/local.d && ln -sf {path} /etc/local.d/isoloom-{name}.start\nrc-update add local default >/dev/null 2>&1 || true\n{path} || true\n\
         fi\n"
    )
}

#[cfg(test)]
mod tests {
    use super::at_boot;

    #[test]
    fn the_script_is_kept_and_run_at_every_boot() {
        let sh = at_boot(
            "routes",
            "Routes to the other networks",
            "ip route replace 10.1.0.0/24 via 10.0.0.1\n",
            true,
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
        assert!(!at_boot("tc", "Link impairment", "tc qdisc\n", false, &[]).contains("if-up.d"));
    }

    #[test]
    fn it_runs_under_sh() {
        let dir = std::env::temp_dir().join(format!("isoloom-boot-{}", std::process::id()));
        // Only the file part: a test can't enable a systemd unit.
        let sh = at_boot("t", "test", "echo ok\n", false, &[]).replace("/etc/isoloom", &dir.display().to_string());
        let file_part: String = sh.split("if command -v systemctl").next().unwrap_or_default().to_string();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{file_part}{}/t.sh", dir.display()))
            .output();
        let _ = std::fs::remove_dir_all(&dir);
        let out = out.expect("sh runs");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "ok\n");
        assert!(out.status.success());
    }
}
