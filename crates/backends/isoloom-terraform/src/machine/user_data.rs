//! A Linux machine that sets itself up from its user data, on a cloud. Terraform makes the VM and
//! nothing else: no SSH from Terraform, no file it uploads. The user data writes the set-up
//! script and a systemd unit running it at boot, then starts the unit; the set-up waits for the
//! project, which the Isothrum runner delivers over SSH once the apply is done (an `ssh.run` step
//! of the run manifest, piping the project's archive into `tar`), then runs the machine's steps.
//!
//! The set-up says how far it got in the ready file, in the `isoloom-ready/1` grammar the runner
//! reads (`running: <step>`, `failed: <step>`, `ready`), and writes its output to
//! [`SETUP_LOG`], whose tail explains a failed step. A `reboot` step ends a part: the set-up
//! records the next part and restarts the machine, and the unit runs that part at the next boot,
//! after the commands a restart undoes (`again`). The same script serves a cloud that runs its
//! user data once (cloud-init) and one that runs it at every boot (Google Cloud's startup
//! script): a set-up that ended (`ready`, or `failed: …`) doesn't run again, and the script it
//! is reading is never rewritten in place.

use isoloom_render::hcl_builder::{Expression, Template, reference};
use isoloom_spec::shell::quote;

use super::cloud_init::READY_FILE;

/// Where the set-up writes its output: the runner shows its tail when a step failed.
pub(crate) const SETUP_LOG: &str = "/var/log/isoloom-setup.log";

/// The mark the runner's delivery leaves once the project is in [`isoloom_render::steps::PROJECT_DIR`].
pub(crate) const DELIVERED: &str = "/var/lib/isoloom/project-delivered";

/// How long the set-up waits for the project, in seconds: the apply of the other machines, then
/// the runner's delivery to each machine in turn.
pub(crate) const DELIVERY_WAIT_S: u32 = 2400;

/// The set-up's own folder: the script, the part it is at.
const DIR: &str = "/var/lib/isoloom";

/// The unit running the set-up at boot.
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
    /// At the start of each part after the first: what a restart undoes (interfaces, routes).
    pub(crate) again: Vec<Template>,
}

/// The expiry, scheduled as a shutdown at every boot when the run has one (`var.expires_at`, in
/// Unix seconds): a machine that outlives its run stops itself, whatever happens to the runner.
/// A VM's whole user data when its only job is to shut itself down at the run's end: `#!/bin/sh`,
/// then the expiry shutdown, scheduled at every boot. A cloud-docker host, set up over SSH, needs
/// no more of its user data.
pub(crate) fn expiry_only() -> Expression {
    let mut t = Template::text("#!/bin/sh\n");
    t.append(expiry_shutdown());
    t.push_text("\n");
    t.into_expression()
}

/// The expiry as a shell line scheduling `shutdown` at `var.expires_at`.
pub(crate) fn expiry_shutdown() -> Template {
    Template::text("e=")
        .with_expr(reference("var.expires_at"))
        .with_text("; if [ -n \"$e\" ]; then m=$(( (e - $(date +%s)) / 60 )); [ \"$m\" -ge 1 ] || m=1; shutdown -h \"+$m\" >/dev/null 2>&1 || true; fi")
}

/// The wait for the runner's delivery of the project: the first step of every set-up that reads
/// the project.
pub(crate) fn project_wait() -> Tracked {
    Tracked::new(
        "waiting for the project (the runner delivers it over SSH)",
        format!(
            "i=0; until [ -f {DELIVERED} ]; do i=$((i+5)); if [ $i -gt {DELIVERY_WAIT_S} ]; then echo 'the project never came: start the lab through the Isothrum runner (run.json), which delivers it over SSH' >&2; exit 1; fi; sleep 5; done"
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

/// The set-up script: done or failed already, it stops; else the commands of every boot, then
/// the part it is at, then the next ones; `ready` at the end.
fn body(s: &SetUp) -> Template {
    let mut t = Template::text(format!(
        "#!/bin/sh\numask 022\ncase \"$(head -n1 {READY_FILE} 2>/dev/null)\" in ready|failed:*) exit 0 ;; esac\n"
    ));
    for c in &s.each_boot {
        t.append(c.clone());
        t.push_text("\n");
    }
    t.push_text(format!("part=$(cat {DIR}/part 2>/dev/null || echo 1)\n"));
    let count = s.parts.len();
    for (n, part) in (1..).zip(&s.parts) {
        t.push_text(format!("if [ \"$part\" = {n} ]; then\n"));
        if n > 1 {
            for c in &s.again {
                t.append(c.clone());
                t.push_text("\n");
            }
        }
        for c in part {
            t.append(tracked(c));
            t.push_text("\n");
        }
        if n < count {
            t.push_text(format!("echo {} > {DIR}/part\nsystemctl reboot\nexit 0\n", n + 1));
        }
        t.push_text("fi\n");
    }
    t.push_text(format!(
        "echo ready > {READY_FILE}\nrm -f {DIR}/part\ncommand -v systemctl >/dev/null && systemctl disable {UNIT} >/dev/null 2>&1\nexit 0\n"
    ));
    t
}

/// The unit running the set-up at boot (after cloud-init or the startup script), its output
/// appended to [`SETUP_LOG`], with no time limit.
fn unit() -> String {
    format!(
        "[Unit]\nDescription=Isoloom: the machine's set-up\nWants=network-online.target\nAfter=network-online.target cloud-final.service google-startup-scripts.service\n\n[Service]\nType=oneshot\nExecStart=/bin/sh {DIR}/setup.sh\nStandardOutput=append:{SETUP_LOG}\nStandardError=append:{SETUP_LOG}\nTimeoutStartSec=0\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

/// The user data: the set-up script and its unit written (each through a new file moved in
/// place, so a set-up reading its script never sees it rewritten), then the unit started without
/// waiting for it. Without systemd, the set-up runs in the background.
#[must_use]
pub(crate) fn script(s: &SetUp) -> Template {
    let mut t = Template::text(format!(
        "#!/bin/sh\n# Isoloom: this machine sets itself up. Its progress is in {READY_FILE} (isoloom-ready/1), its output in {SETUP_LOG}. The Isothrum runner delivers the project over SSH.\nmkdir -p {DIR}\ncat > {DIR}/setup.sh.new <<'ISOLOOM_SETUP'\n"
    ));
    t.append(body(s));
    t.push_text(format!(
        "ISOLOOM_SETUP\nmv -f {DIR}/setup.sh.new {DIR}/setup.sh\nif command -v systemctl >/dev/null; then\ncat > /etc/systemd/system/{UNIT} <<'ISOLOOM_UNIT'\n{}ISOLOOM_UNIT\nsystemctl daemon-reload\nsystemctl enable {UNIT} >/dev/null 2>&1\nsystemctl start --no-block {UNIT}\nelse\nnohup sh {DIR}/setup.sh >> {SETUP_LOG} 2>&1 &\nfi\n",
        unit()
    ));
    t
}

/// [`script`] as an expression.
#[must_use]
pub(crate) fn expression(s: &SetUp) -> Expression {
    script(s).into_expression()
}

#[cfg(test)]
mod tests {
    use isoloom_render::hcl_builder::Piece;

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
        let reboot = s.find("echo 2 > /var/lib/isoloom/part\nsystemctl reboot").unwrap();
        let again = s.find("if [ \"$part\" = 2 ]; then\nip link set eth1 up\n").unwrap();
        assert!(reboot < again, "{s}");
        assert_eq!(s.matches("ip link set eth1 up").count(), 1, "not in the first part: {s}");
    }

    #[test]
    fn a_set_up_that_ended_never_runs_again_and_its_script_is_moved_in_place() {
        let s = text(&script(&SetUp::default()));
        assert!(s.contains("in ready|failed:*) exit 0"), "{s}");
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
}
