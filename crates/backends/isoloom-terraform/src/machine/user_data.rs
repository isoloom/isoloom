//! A Linux machine that sets itself up from its user data, on a cloud. Terraform makes the VM and
//! nothing else: no SSH from Terraform, no file it uploads. The user data writes the set-up
//! script and a systemd unit running it at every boot, then starts the unit; the set-up waits
//! for the project, which the runner delivers over SSH once the apply is done (an `ssh.run` step
//! of the run manifest, piping the project's archive into `tar`), then runs the machine's steps.
//!
//! The set-up says how far it got in the ready file, in the `isoloom-ready/1` grammar the runner
//! reads (`running: <step>`, `failed: <step>`, `ready`), and writes its output to
//! [`SETUP_LOG`], whose tail explains a failed step. A `reboot` step ends a part: the set-up
//! records the next part and restarts the machine, and the unit runs that part at the next boot.
//! At every boot, before anything else, it schedules the expiry again (`each_boot`); at every
//! boot after the first part (a `reboot` step's, or any later restart once the set-up is
//! `ready`), it runs the commands a restart undoes (`again`). A set-up that ended (`ready`, or
//! `failed: …`) runs no step again. The same script serves a cloud that runs its user data once
//! (cloud-init) and one that runs it at every boot (Google Cloud's startup script), and the
//! script it is reading is never rewritten in place.

use isoloom_render::boot::{self, Trigger};
use isoloom_render::configure::cloud_init::gzip;

use crate::cloud::user_data_limit::Limit;
use crate::hcl_builder::{Expression, Piece, Template, reference};
use isoloom_render::run_manifest::RUNNER;
use isoloom_render::steps::wait_for_mark;
use isoloom_spec::model::Target;
use isoloom_spec::shell::quote;

use crate::{Error, Result};
use isoloom_render::steps::READY_FILE;

/// Where the set-up writes its output: the runner shows its tail when a step failed.
pub(crate) const SETUP_LOG: &str = "/var/log/isoloom-setup.log";

/// The mark the runner's delivery leaves once the project is in [`isoloom_render::steps::PROJECT_DIR`].
pub(crate) const DELIVERED: &str = isoloom_spec::linux_state_path!("/project-delivered");

/// How long the set-up waits for the project, in seconds: the apply of the other machines, then
/// the runner's delivery to each machine in turn.
pub(crate) const DELIVERY_WAIT_S: u32 = 2400;

/// The set-up's own folder: the script, the part it is at.
const DIR: &str = isoloom_spec::linux::STATE_DIR;

/// The unit running the set-up at every boot.
const UNIT: &str = "isoloom-setup.service";

/// A command of the set-up, under the name the ready file gives it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tracked {
    /// What the ready file calls it (`running: <label>`).
    pub(crate) label: String,
    /// The command, run as root.
    pub(crate) command: Template,
}

impl Tracked {
    /// `command` as the step `label`.
    pub(crate) fn new(label: impl Into<String>, command: impl Into<Template>) -> Tracked {
        Tracked {
            label: label.into(),
            command: command.into(),
        }
    }
}

/// A machine's set-up, as its user data runs it.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SetUp {
    /// At every boot, before anything else: the expiry's shutdown, scheduled again.
    pub(crate) each_boot: Vec<Template>,
    /// The parts between `reboot` steps, in order.
    pub(crate) parts: Vec<Vec<Tracked>>,
    /// At every boot after the first part, before the steps: what a restart undoes
    /// (interfaces, routes).
    pub(crate) again: Vec<Template>,
}

/// Where a host whose user data only schedules its expiry keeps the run's end (Unix seconds, or
/// nothing), for its boot script to read.
const EXPIRES_AT: &str = isoloom_spec::linux_state_path!("/expires-at");

/// The shutdown at the run's end, `$e` (Unix seconds, empty without an end): in `$m` minutes,
/// at least one.
const SHUTDOWN_AT_E: &str = "if [ -n \"$e\" ]; then m=$(( (e - $(date +%s)) / 60 )); [ \"$m\" -ge 1 ] || m=1; shutdown -h \"+$m\" >/dev/null 2>&1 || true; fi";

/// A VM's whole user data when its only job is to shut itself down at the run's end
/// (`var.expires_at`, in Unix seconds): `#!/bin/sh`, then the end kept in a file and a boot
/// script scheduling the shutdown from it, now and at every boot (a cloud that runs its user
/// data once still reschedules it after a restart). A cloud-docker host, set up over SSH, needs
/// no more of its user data.
pub(crate) fn expiry_only() -> Expression {
    expiry_only_script().into_expression()
}

/// [`expiry_only`] as a template.
fn expiry_only_script() -> Template {
    let mut t = Template::text(format!("#!/bin/sh\nmkdir -p {DIR}\necho "));
    t.push_expr(reference("var.expires_at"));
    t.push_text(format!(" > {EXPIRES_AT}\n"));
    t.push_text(boot::at_boot(
        "expiry",
        "The run's end: the machine shuts itself down",
        &format!("e=$(cat {EXPIRES_AT} 2>/dev/null); {SHUTDOWN_AT_E}\n"),
        Trigger::Boot,
        &[],
    ));
    t
}

/// The expiry as a shell line scheduling `shutdown` at `var.expires_at`.
pub(crate) fn expiry_shutdown() -> Template {
    Template::text("e=")
        .with_expr(reference("var.expires_at"))
        .with_text(format!("; {SHUTDOWN_AT_E}"))
}

/// The wait for the runner's delivery of the project: the first step of every set-up that reads
/// the project.
pub(crate) fn project_wait() -> Tracked {
    Tracked::new(
        "waiting for the project (the runner delivers it over SSH)",
        wait_for_mark(
            DELIVERED,
            DELIVERY_WAIT_S,
            &format!("waiting for the project, which {RUNNER} delivers over SSH"),
            &format!("the project never came: start the environment through {RUNNER}, which delivers it over SSH"),
        ),
    )
}

/// `command` as the step `label` of the ready file: its name first, then `failed: <label>` and
/// the end of the set-up when it fails.
fn tracked(t: &Tracked) -> Template {
    let running = quote(&format!("running: {}", t.label));
    let failed = quote(&format!("failed: {}", t.label));
    let mut line = Template::text(format!("echo {running} > {READY_FILE}; {{ "));
    line.append(t.command.clone());
    line.push_text(format!("; }} || {{ echo {failed} > {READY_FILE}; exit 1; }}"));
    line
}

/// The set-up script: the commands of every boot; after the first part, what a restart undid;
/// then, unless it ended already, the part it is at and the next ones, `ready` at the end.
fn body(s: &SetUp) -> Template {
    let mut t = Template::text("#!/bin/sh\numask 022\n");
    for c in &s.each_boot {
        t.append(c.clone());
        t.push_text("\n");
    }
    t.push_text(format!(
        "state=$(head -n1 {READY_FILE} 2>/dev/null)\npart=$(cat {DIR}/part 2>/dev/null || echo 1)\n"
    ));
    if !s.again.is_empty() {
        t.push_text("if [ \"$state\" = ready ] || [ \"$part\" != 1 ]; then\n");
        for c in &s.again {
            t.append(c.clone());
            t.push_text("\n");
        }
        t.push_text("fi\n");
    }
    t.push_text("case \"$state\" in ready|failed:*) exit 0 ;; esac\n");
    let count = s.parts.len();
    for (n, part) in (1..).zip(&s.parts) {
        t.push_text(format!("if [ \"$part\" = {n} ]; then\n"));
        for c in part {
            t.append(tracked(c));
            t.push_text("\n");
        }
        if n < count {
            // The next part runs at the next boot, after `again`.
            t.push_text(format!("echo {} > {DIR}/part\nsystemctl reboot\nexit 0\n", n + 1));
        }
        t.push_text("fi\n");
    }
    t.push_text(format!("echo ready > {READY_FILE}\nrm -f {DIR}/part\nexit 0\n"));
    t
}

/// The unit running the set-up at every boot (after cloud-init or the startup script), its
/// output appended to [`SETUP_LOG`], with no time limit. It stays enabled once the set-up is
/// `ready`: the expiry and what a restart undoes come back after every restart.
fn unit() -> String {
    format!(
        "[Unit]\nDescription=Isoloom: the machine's set-up\nWants=network-online.target\nAfter=network-online.target cloud-final.service google-startup-scripts.service\n\n[Service]\nType=oneshot\nExecStart=/bin/sh {DIR}/setup.sh\nStandardOutput=append:{SETUP_LOG}\nStandardError=append:{SETUP_LOG}\nTimeoutStartSec=0\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

/// The user data: the set-up script and its unit written (each through a new file moved in
/// place, so a set-up reading its script never sees it rewritten), then the unit started without
/// waiting for it. Without systemd, the set-up runs in the background. The script is readable by
/// root alone (it holds the sensitive inputs in base64, and on the controller its private key and
/// the inventory): a fresh file, written under umask 077.
#[must_use]
pub(crate) fn script(s: &SetUp) -> Template {
    let mut t = Template::text(format!(
        "#!/bin/sh\n# Isoloom: this machine sets itself up. Its progress is in {READY_FILE} (isoloom-ready/1), its output in {SETUP_LOG}. The project comes over SSH from {RUNNER}.\nmkdir -p {DIR}\nrm -f {DIR}/setup.sh.new\n(umask 077 && cat > {DIR}/setup.sh.new) <<'ISOLOOM_SETUP'\n"
    ));
    t.append(body(s));
    t.push_text(format!(
        "ISOLOOM_SETUP\nmv -f {DIR}/setup.sh.new {DIR}/setup.sh\nif command -v systemctl >/dev/null; then\ncat > /etc/systemd/system/{UNIT} <<'ISOLOOM_UNIT'\n{}ISOLOOM_UNIT\nsystemctl daemon-reload\nsystemctl enable {UNIT} >/dev/null 2>&1\nsystemctl start --no-block {UNIT}\nelse\nnohup sh {DIR}/setup.sh >> {SETUP_LOG} 2>&1 &\nfi\n",
        unit()
    ));
    t
}

/// How a cloud receives a machine's user data: its cap, and whether the module gzips the script
/// (AWS's `base64gzip`: AWS counts the gzip's bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Capped {
    /// The cloud's cap.
    pub(crate) limit: Limit,
    /// Whether the module sends the script gzipped.
    pub(crate) gzipped: bool,
}

/// [`script`] as an expression on a cloud that caps its user data: refused, naming `who` (as a
/// refusal reads: ``machine `web` ``, `the controller`), when it is over the cap as the cloud
/// counts it.
///
/// What is measured is the text Isoloom writes; the values Terraform interpolates at apply (the
/// launch's inputs, the expiry, an interface's address) are left out, so a refusal is certain,
/// and a script within a few bytes of the cap may still be refused by the cloud.
///
/// # Errors
///
/// [`Error::Refused`] naming `who`, its size and the cap.
pub(crate) fn capped(s: &SetUp, who: &str, cap: Capped) -> Result<Expression> {
    let t = script(s);
    let text: String = t
        .pieces()
        .filter_map(|p| match p {
            Piece::Text(text) => Some(text),
            Piece::Expr(_) => None,
        })
        .collect();
    let document = if cap.gzipped { gzip(&text) } else { text.into_bytes() };
    cap.limit.fits(&document).map_err(|too_big| Error::Refused {
        target: Target::CloudVm,
        reason: format!("{who}: {too_big}"),
    })?;
    Ok(t.into_expression())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &Template) -> String {
        t.pieces()
            .map(|p| match p {
                Piece::Text(s) => s.to_owned(),
                Piece::Expr(_) => "<expr>".to_owned(),
            })
            .collect()
    }

    fn two_parts() -> SetUp {
        SetUp {
            each_boot: vec![expiry_shutdown()],
            parts: vec![
                vec![project_wait(), Tracked::new("provision/web.sh", "sh -c 'sh provision/web.sh'")],
                vec![Tracked::new("after the reboot", "true")],
            ],
            again: vec![Template::text("ip link set eth1 up")],
        }
    }

    #[test]
    fn the_set_up_waits_for_the_project_then_says_each_step_in_the_ready_file() {
        let s = text(&script(&two_parts()));
        let wait = s.find(DELIVERED).unwrap();
        let step = s.find("echo 'running: provision/web.sh' > /var/lib/isoloom/ready; { sh -c 'sh provision/web.sh'; } || { echo 'failed: provision/web.sh' > /var/lib/isoloom/ready; exit 1; }").unwrap();
        assert!(wait < step, "{s}");
        assert!(s.contains("echo ready > /var/lib/isoloom/ready"), "{s}");
    }

    #[test]
    fn a_reboot_ends_a_part_and_the_next_boot_runs_what_a_restart_undid_first() {
        let s = text(&script(&two_parts()));
        let again = s
            .find("if [ \"$state\" = ready ] || [ \"$part\" != 1 ]; then\nip link set eth1 up\nfi\n")
            .unwrap();
        let reboot = s.find("echo 2 > /var/lib/isoloom/part\nsystemctl reboot").unwrap();
        let second = s.find("if [ \"$part\" = 2 ]; then\n").unwrap();
        assert!(again < reboot && reboot < second, "{s}");
        assert_eq!(s.matches("ip link set eth1 up").count(), 1, "{s}");
    }

    #[test]
    fn every_boot_after_the_set_up_ended_schedules_the_expiry_and_undoes_a_restart() {
        let s = text(&script(&two_parts()));
        let end = s.find("case \"$state\" in ready|failed:*) exit 0 ;; esac").unwrap();
        assert!(s.find("e=<expr>").unwrap() < end, "{s}");
        assert!(s.find("ip link set eth1 up").unwrap() < end, "{s}");
        assert!(!s.contains("systemctl disable"), "the unit stays enabled: {s}");
        assert!(s.contains("systemctl enable isoloom-setup.service"), "{s}");
    }

    #[test]
    fn under_sh_the_steps_run_once_and_what_a_restart_undid_at_every_later_boot() {
        use isoloom_render::test_support::{posix_sh, sh_path};
        if posix_sh().is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("isoloom-setup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = sh_path(&dir.join("log"));
        let set_up = SetUp {
            each_boot: vec![Template::text(format!("echo boot >> {log}"))],
            parts: vec![vec![Tracked::new("step", format!("echo step >> {log}"))]],
            again: vec![Template::text(format!("echo again >> {log}"))],
        };
        let sh = text(&body(&set_up)).replace(DIR, &sh_path(&dir));
        let boot = || {
            let out = posix_sh().unwrap().arg("-c").arg(&sh).output().unwrap();
            assert!(out.status.success(), "{out:?}");
        };
        boot();
        boot();
        boot();
        let got = std::fs::read_to_string(dir.join("log")).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got, "boot\nstep\nboot\nagain\nboot\nagain\n");
    }

    #[test]
    fn a_host_that_only_expires_reschedules_it_at_every_boot() {
        let s = text(&expiry_only_script());
        assert!(
            s.starts_with("#!/bin/sh\nmkdir -p /var/lib/isoloom\necho <expr> > /var/lib/isoloom/expires-at\n"),
            "{s}"
        );
        assert!(s.contains("e=$(cat /var/lib/isoloom/expires-at 2>/dev/null); if [ -n \"$e\" ]"), "{s}");
        assert!(s.contains("systemctl enable isoloom-expiry.service"), "a boot unit: {s}");
    }

    #[test]
    fn a_set_up_that_ended_never_runs_again_and_its_script_is_moved_in_place() {
        let s = text(&script(&SetUp::default()));
        assert!(s.contains("in ready|failed:*) exit 0"), "{s}");
        assert!(!s.contains("= ready ] ||"), "nothing to do again: {s}");
        assert!(s.contains("mv -f /var/lib/isoloom/setup.sh.new /var/lib/isoloom/setup.sh"), "{s}");
        assert!(s.contains("systemctl start --no-block isoloom-setup.service"), "{s}");
        assert!(s.starts_with("#!/bin/sh\n"), "cloud-init runs user data as a script only with its #!: {s}");
    }

    #[test]
    fn the_expiry_is_scheduled_from_the_runs_end_at_every_boot() {
        let e = text(&expiry_shutdown());
        assert!(e.starts_with("e=<expr>;"), "{e}");
        assert!(e.contains("shutdown -h \"+$m\""), "{e}");
        let s = text(&script(&two_parts()));
        assert!(s.find("e=<expr>").unwrap() < s.find("part=$(cat").unwrap(), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn the_set_up_script_is_readable_by_root_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("isoloom-setup-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        // A file left from an earlier write, readable by anyone, isn't reused.
        std::fs::write(dir.join("setup.sh.new"), "old").expect("a stale file");
        std::fs::set_permissions(dir.join("setup.sh.new"), std::fs::Permissions::from_mode(0o644)).expect("its mode");
        let s = text(&script(&SetUp::default()));
        let (written, _) = s.split_once("mv -f").expect("the script is moved in place");
        let written = written.replace(DIR, &dir.to_string_lossy());
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("umask 022\n{written}"))
            .status()
            .expect("sh runs");
        let mode = std::fs::metadata(dir.join("setup.sh.new")).expect("the script").permissions().mode();
        std::fs::remove_dir_all(&dir).expect("the temporary folder goes");
        assert!(status.success());
        assert_eq!(mode & 0o777, 0o600);
    }
}
