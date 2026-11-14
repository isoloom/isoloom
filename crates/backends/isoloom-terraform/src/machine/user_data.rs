//! A Linux machine that sets itself up from its user data, on a cloud. The user data writes the
//! set-up script and a systemd unit running it at every boot, then starts the unit; the set-up
//! waits for the project, which Terraform brings in over SSH once the machine is up
//! (`crate::ready`: the project's archive uploaded and unpacked, then the mark the set-up waits
//! for), then runs the machine's steps.
//!
//! The set-up says how far it got in the ready file, in the `isoloom-ready/1` grammar the same
//! resource reads to end the apply (`running: <step>`, `failed: <step>`, `ready`), and writes its
//! output to [`SETUP_LOG`], whose tail explains a failed step. A `reboot` step ends a part: the set-up
//! records the next part and restarts the machine, and the unit runs that part at the next boot.
//! At every boot, before anything else, it schedules the expiry again (`each_boot`); at every
//! boot after the first part (a `reboot` step's, or any later restart once the set-up is
//! `ready`), it runs the commands a restart undoes (`again`). A set-up that ended (`ready`, or
//! `failed: …`) runs no step again. The same script serves a cloud that runs its user data once
//! (cloud-init) and one that runs it at every boot (Google Cloud's startup script), and the
//! script it is reading is never rewritten in place.

use isoloom_render::configure::cloud_init::gzip;

use crate::cloud::user_data_limit::Limit;
use crate::hcl_builder::{Expression, Piece, Template, reference};
use isoloom_render::steps::wait_for_mark;
use isoloom_spec::model::Target;

use crate::{Error, Result};
use isoloom_manifest::ReadyLine;
use isoloom_render::steps::{READY_FILE, failed_pattern, ready_mark};

/// Where the set-up writes its output: Terraform's wait shows its tail when a step failed.
pub(crate) const SETUP_LOG: &str = "/var/log/isoloom-setup.log";

/// The mark Terraform's delivery leaves once the project is in [`isoloom_render::steps::PROJECT_DIR`].
pub(crate) const DELIVERED: &str = isoloom_spec::linux_state_path!("/project-delivered");

/// How long the set-up waits for the project, in seconds: the apply of the other machines, then
/// Terraform's delivery (it waits for SSH to answer first).
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

/// The shutdown at the run's end, `$e` (Unix seconds, empty without an end): in `$m` minutes,
/// at least one.
const SHUTDOWN_AT_E: &str = "if [ -n \"$e\" ]; then m=$(( (e - $(date +%s)) / 60 )); [ \"$m\" -ge 1 ] || m=1; shutdown -h \"+$m\" >/dev/null 2>&1 || true; fi";

/// The expiry as a shell line scheduling `shutdown` at `var.expires_at`.
pub(crate) fn expiry_shutdown() -> Template {
    Template::text("e=")
        .with_expr(reference("var.expires_at"))
        .with_text(format!("; {SHUTDOWN_AT_E}"))
}

/// The wait for Terraform's delivery of the project: the first step of every set-up that reads
/// the project.
pub(crate) fn project_wait() -> Tracked {
    Tracked::new(
        "waiting for the project (Terraform brings it in over SSH)",
        wait_for_mark(
            DELIVERED,
            DELIVERY_WAIT_S,
            "waiting for the project, which Terraform brings in over SSH during its apply",
            "the project never came: Terraform brings it in over SSH during its apply (its terraform_data.isoloom_ready_<machine>), with ssh_private_key_file",
        ),
    )
}

/// `command` as the step `label` of the ready file: its name first, then `failed: <label>` and
/// the end of the set-up when it fails.
fn tracked(t: &Tracked) -> Template {
    let mut line = Template::text(format!("{}; {{ ", ready_mark(&ReadyLine::running(&t.label))));
    line.append(t.command.clone());
    line.push_text(format!("; }} || {{ {}; exit 1; }}", ready_mark(&ReadyLine::failed(&t.label))));
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
        t.push_text(format!("if [ \"$state\" = {} ] || [ \"$part\" != 1 ]; then\n", ReadyLine::READY));
        for c in &s.again {
            t.append(c.clone());
            t.push_text("\n");
        }
        t.push_text("fi\n");
    }
    t.push_text(format!("case \"$state\" in {}|{}) exit 0 ;; esac\n", ReadyLine::READY, failed_pattern()));
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
    t.push_text(format!("{}\nrm -f {DIR}/part\nexit 0\n", ready_mark(&ReadyLine::Ready)));
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
    let body = body(s);
    // The set-up holds the spec's values: its terminator is no line of it. An interpolated value
    // (an address, a key the module reads) is never a line of its own there.
    let known: String = body
        .pieces()
        .map(|p| match p {
            Piece::Text(t) => t,
            Piece::Expr(_) => "\u{0}",
        })
        .collect();
    let tag = isoloom_spec::shell::here_tag("ISOLOOM_SETUP", &known);
    let mut t = Template::text(format!(
        "#!/bin/sh\n# Isoloom: this machine sets itself up. Its progress is in {READY_FILE} (isoloom-ready/1), its output in {SETUP_LOG}. The project comes over SSH from Terraform.\nmkdir -p {DIR}\nrm -f {DIR}/setup.sh.new\n(umask 077 && cat > {DIR}/setup.sh.new) <<'{tag}'\n"
    ));
    t.append(body);
    t.push_text(format!(
        "{tag}\nmv -f {DIR}/setup.sh.new {DIR}/setup.sh\nif command -v systemctl >/dev/null; then\ncat > /etc/systemd/system/{UNIT} <<'ISOLOOM_UNIT'\n{}ISOLOOM_UNIT\nsystemctl daemon-reload\nsystemctl enable {UNIT} >/dev/null 2>&1\nsystemctl start --no-block {UNIT}\nelse\nnohup sh {DIR}/setup.sh >> {SETUP_LOG} 2>&1 &\nfi\n",
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
        let end = s.find("case \"$state\" in ready|\"failed: \"*) exit 0 ;; esac").unwrap();
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
        let dir = tempfile::Builder::new().prefix("isoloom-setup-").tempdir().unwrap();
        let dir = dir.path();
        let log = sh_path(&dir.join("log"));
        let set_up = SetUp {
            each_boot: vec![Template::text(format!("echo boot >> {log}"))],
            parts: vec![vec![Tracked::new("step", format!("echo step >> {log}"))]],
            again: vec![Template::text(format!("echo again >> {log}"))],
        };
        let sh = text(&body(&set_up)).replace(DIR, &sh_path(dir));
        let boot = || {
            let out = posix_sh().unwrap().arg("-c").arg(&sh).output().unwrap();
            assert!(out.status.success(), "{out:?}");
        };
        boot();
        boot();
        boot();
        let got = std::fs::read_to_string(dir.join("log")).unwrap();
        assert_eq!(got, "boot\nstep\nboot\nagain\nboot\nagain\n");
    }

    #[test]
    fn under_sh_a_set_up_ended_on_exactly_the_lines_the_ready_grammar_reads_as_ended() {
        use isoloom_render::test_support::posix_sh;
        if posix_sh().is_none() {
            return;
        }
        let s = text(&script(&SetUp::default()));
        let case = s.lines().find(|l| l.starts_with("case \"$state\" in")).unwrap();
        let lines = [
            ReadyLine::Ready.to_string(),
            ReadyLine::failed("a: b.sh").to_string(),
            ReadyLine::running("a: b.sh").to_string(),
            "failed:a".to_owned(),
            "readyish".to_owned(),
        ];
        for line in lines {
            let sh = format!("state={}\n{case}\nexit 1", isoloom_spec::shell::quote(&line));
            let ended = posix_sh().unwrap().arg("-c").arg(&sh).status().unwrap().success();
            let read = ReadyLine::parse(&line);
            assert_eq!(ended, matches!(read, Some(ReadyLine::Ready | ReadyLine::Failed { .. })), "{line}: {case}");
        }
    }

    #[test]
    fn a_set_up_that_ended_never_runs_again_and_its_script_is_moved_in_place() {
        let s = text(&script(&SetUp::default()));
        assert!(s.contains("in ready|\"failed: \"*) exit 0"), "{s}");
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
        let dir = tempfile::Builder::new().prefix("isoloom-setup-mode-").tempdir().expect("a temporary folder");
        let dir = dir.path();
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
        assert!(status.success());
        assert_eq!(mode & 0o777, 0o600);
    }
}
