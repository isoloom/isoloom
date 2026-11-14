//! A machine's readiness, inside the module: per machine that sets itself up after Terraform
//! made it (a cloud machine from its user data, a Proxmox VM from its cloud-init), one
//! `terraform_data` resource that signs in over SSH (through the router on a server) and waits
//! for its set-up's end, so `terraform apply` ends only once every machine is ready, and fails,
//! with the end of the set-up's output, when one stopped.
//!
//! The wait reads the ready file the set-up writes (`isoloom-ready/1`: `running: <step>`,
//! `failed: <step>`, `ready`), after `cloud-init status --wait` where cloud-init runs. A set-up
//! that restarts the machine (a `reboot` step) drops the SSH session: the wait is written once
//! per restart, each one but the last going on when its session drops (`on_failure =
//! continue`), and the next one signing in again once the machine is back. A failed set-up fails
//! each of them, so the last one fails the apply.
//!
//! On a cloud the same resource first brings the project in (archived where Terraform runs, `tar`
//! honouring `.isoloomignore`, uploaded, unpacked into `/opt/isoloom`, then the mark the set-up
//! waits for).
//!
//! What closes a network once the machines are set up (`crate::seal`) waits for these
//! resources ([`all`]).

use isoloom_plan::Step;
use isoloom_render::steps::{self, AsRoot, READY_FILE};
use isoloom_spec::linux::STATE_DIR;
use isoloom_spec::shell::quote;

use crate::hcl_builder::{Block, Expression, list, reference, string};
use crate::machine::provisioner::{self, Connection, Upload};
use crate::module::ident;

/// How many lines of the set-up's output a failed wait prints.
const LOG_LINES: u32 = 40;

/// How often the wait reads the ready file, in seconds.
const POLL_S: u32 = 5;

/// A machine Terraform waits for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Wait {
    /// The machine's name.
    pub(crate) machine: String,
    /// How Terraform signs in (SSH, through the router on a server).
    pub(crate) connection: Connection,
    /// What makes the wait again when it changes: the machine's id.
    pub(crate) replace_on: Expression,
    /// How a command run as the login user gets root.
    pub(crate) as_root: AsRoot,
    /// The file the set-up writes its output to, whose end a failed wait prints.
    pub(crate) log: &'static str,
    /// How long the set-up may take, in seconds.
    pub(crate) within_s: u32,
    /// How many times the set-up restarts the machine.
    pub(crate) reboots: usize,
    /// Whether the project is brought in first: the set-up waits for it.
    pub(crate) project: bool,
    /// Isoloom's outputs brought in with the project (paths under the project's folder, which
    /// its archive leaves out): the Compose file a Docker host runs.
    pub(crate) outputs: Vec<String>,
}

/// Where Isoloom's outputs a machine gets are uploaded, archived.
const OUTPUTS_ARCHIVE: &str = "/tmp/isoloom-outputs.tgz";

/// The resource's name for machine `machine`.
fn name(machine: &str) -> String {
    format!("isoloom_ready_{}", ident(machine))
}

/// `terraform_data.isoloom_ready_<machine>`: what waits for machine `machine`.
pub(crate) fn of(machine: &str) -> Expression {
    reference(&format!("terraform_data.{}", name(machine)))
}

/// Every wait of `machines`, for a resource that must come after them (the seal).
pub(crate) fn all<'a>(machines: impl IntoIterator<Item = &'a str>) -> Vec<Expression> {
    machines.into_iter().map(of).collect()
}

/// How many times a machine's set-up restarts it: its `reboot` steps.
pub(crate) fn reboots(lifecycle: &[Step]) -> usize {
    lifecycle.iter().filter(|s| **s == Step::Reboot).count()
}

impl Wait {
    /// The `terraform_data` block.
    pub(crate) fn block(self) -> Block {
        let mut b = Block::builder("resource")
            .add_label("terraform_data")
            .add_label(name(&self.machine).as_str())
            .add_attribute(("triggers_replace", list([self.replace_on.clone()])))
            .add_block(provisioner::connection(&self.connection));
        if self.project {
            let upload = Upload::Project {
                id: format!("ready_{}", ident(&self.machine)),
                destination: steps::PROJECT_ARCHIVE,
            };
            b = b.add_blocks(provisioner::upload(upload));
            if !self.outputs.is_empty() {
                b = b.add_blocks(outputs_upload(&self.machine, &self.outputs));
            }
            b = b.add_block(remote_exec(&self.deliver(), false));
        }
        let wait = self.wait();
        for _ in 0..self.reboots {
            b = b.add_block(remote_exec(&wait, true));
        }
        b.add_block(remote_exec(&wait, false)).build()
    }

    /// The project unpacked into `/opt/isoloom` (Isoloom's outputs it gets over it), then the
    /// mark the set-up waits for.
    fn deliver(&self) -> String {
        let outputs = if self.outputs.is_empty() {
            String::new()
        } else {
            format!(" && tar -xzf {OUTPUTS_ARCHIVE} -C {} && rm -f {OUTPUTS_ARCHIVE}", steps::PROJECT_DIR)
        };
        format!(
            "{}sh -c {}",
            self.as_root.prefix(),
            quote(&format!(
                "{}{outputs} && mkdir -p {STATE_DIR} && touch {}",
                steps::project_unpack(steps::Archive::Uploaded, steps::TopFolder::Stripped),
                crate::machine::user_data::DELIVERED
            ))
        )
    }

    /// The wait: cloud-init's end where it runs, then the ready file read every [`POLL_S`]
    /// seconds until it says `ready` (done) or `failed: <step>` (the set-up's output's end
    /// printed, and a failure), for at most `within_s` seconds.
    fn wait(&self) -> String {
        let sudo = self.as_root.prefix();
        let who = &self.machine;
        let tail = format!("{sudo}tail -n {LOG_LINES} {} 2>/dev/null", self.log);
        let script = format!(
            "t=0\n\
             if command -v cloud-init >/dev/null 2>&1; then {sudo}cloud-init status --wait >/dev/null 2>&1 || true; fi\n\
             while :; do\n\
             s=$({sudo}head -n1 {READY_FILE} 2>/dev/null)\n\
             case \"$s\" in\n\
             {ready}) echo \"{who}: set up\"; exit 0 ;;\n\
             {failed}) echo \"{who}: its set-up stopped ($s); the end of {log}:\"; {tail}; exit 1 ;;\n\
             esac\n\
             if [ \"$t\" -ge {within} ]; then echo \"{who}: not set up after {within} s (${{s:-no ready file}}); the end of {log}:\"; {tail}; exit 1; fi\n\
             sleep {POLL_S}; t=$((t + {POLL_S}))\n\
             done",
            ready = isoloom_manifest::ReadyLine::READY,
            failed = steps::failed_pattern(),
            log = self.log,
            within = self.within_s,
        );
        format!("sh -c {}", quote(&script))
    }
}

/// Isoloom's outputs `paths` (under the project's folder, `local.root`) archived where Terraform
/// runs, beside the module, then uploaded to [`OUTPUTS_ARCHIVE`].
fn outputs_upload(machine: &str, paths: &[String]) -> Vec<Block> {
    use crate::hcl_builder::{Template, call};
    let archive = format!("/.isoloom-outputs-{}.tgz", ident(machine));
    let mut command = Template::text("tar -czf \"")
        .with_expr(reference("path.module"))
        .with_text(format!("{archive}\" -C \""))
        .with_expr(call("abspath", [reference("local.root")]))
        .with_text("\"");
    for p in paths {
        // Isoloom's own folders: nothing to quote, on any shell `local-exec` runs.
        command = command.with_text(format!(" {p}"));
    }
    vec![
        Block::builder("provisioner")
            .add_label("local-exec")
            .add_attribute(("command", command.into_expression()))
            .build(),
        Block::builder("provisioner")
            .add_label("file")
            .add_attribute((
                "source",
                Template::new().with_expr(reference("path.module")).with_text(archive).into_expression(),
            ))
            .add_attribute(("destination", string(OUTPUTS_ARCHIVE)))
            .build(),
    ]
}

/// A `remote-exec` running `command`; going on when it fails (`continue`: a restart dropped its
/// session).
fn remote_exec(command: &str, continue_on_failure: bool) -> Block {
    let b = Block::builder("provisioner")
        .add_label("remote-exec")
        .add_attribute(("inline", list([string(command)])));
    if continue_on_failure {
        b.add_attribute(("on_failure", reference("continue"))).build()
    } else {
        b.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcl_builder::Module;
    use isoloom_render::test_support::posix_sh;

    fn wait(reboots: usize, project: bool) -> Wait {
        Wait {
            machine: "web".into(),
            connection: Connection::Ssh {
                host: reference("aws_instance.web.public_ip"),
                user: "admin".into(),
            },
            replace_on: reference("aws_instance.web.id"),
            as_root: AsRoot::Sudo,
            log: "/var/log/isoloom-setup.log",
            within_s: 2400,
            reboots,
            project,
            outputs: Vec::new(),
        }
    }

    fn rendered(w: Wait) -> String {
        let mut m = Module::default();
        m.block(w.block());
        m.render().unwrap()
    }

    #[test]
    fn the_wait_signs_in_with_the_operators_key_and_is_made_again_with_the_machine() {
        let tf = rendered(wait(0, false));
        assert!(tf.contains("resource \"terraform_data\" \"isoloom_ready_web\""), "{tf}");
        assert!(tf.contains("triggers_replace = [aws_instance.web.id]"), "{tf}");
        assert!(tf.contains("private_key = file(pathexpand(var.ssh_private_key_file))"), "{tf}");
        assert_eq!(tf.matches("provisioner \"remote-exec\"").count(), 1, "{tf}");
        assert!(!tf.contains("on_failure"), "{tf}");
        assert!(!tf.contains("provisioner \"file\""), "no project: {tf}");
    }

    #[test]
    fn a_restart_of_the_set_up_is_one_more_wait_going_on_when_its_session_drops() {
        let tf = rendered(wait(2, false));
        assert_eq!(tf.matches("provisioner \"remote-exec\"").count(), 3, "{tf}");
        assert_eq!(tf.matches("on_failure = continue").count(), 2, "{tf}");
        let (_, last) = tf.rsplit_once("provisioner \"remote-exec\"").unwrap();
        assert!(!last.contains("on_failure"), "the last wait fails the apply: {tf}");
    }

    #[test]
    fn the_project_comes_in_before_the_wait_then_the_mark_the_set_up_waits_for() {
        let tf = rendered(wait(0, true));
        let upload = tf.find("provisioner \"file\"").unwrap();
        let deliver = tf.find("touch /var/lib/isoloom/project-delivered").unwrap();
        let wait = tf.find("cloud-init status --wait").unwrap();
        assert!(upload < deliver && deliver < wait, "{tf}");
        assert!(tf.contains("tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom --strip-components=1"), "{tf}");
    }

    /// The wait under `sh`, against a ready file it reads: `ready` ends it well, `failed: …`
    /// fails it with the log's end, and no file until the time is up fails it too.
    #[test]
    fn under_sh_the_wait_ends_on_exactly_the_lines_that_end_a_set_up() {
        if posix_sh().is_none() {
            return;
        }
        let dir = tempfile::Builder::new().prefix("isoloom-ready-").tempdir().unwrap();
        let ready = dir.path().join("ready");
        let log = dir.path().join("log");
        std::fs::write(&log, "step one\nstep two broke\n").unwrap();
        let mut w = wait(0, false);
        w.as_root = AsRoot::Root;
        w.within_s = 0;
        let script = w
            .wait()
            .replace(READY_FILE, &ready.to_string_lossy())
            .replace("/var/log/isoloom-setup.log", &log.to_string_lossy());
        let run = || posix_sh().unwrap().arg("-c").arg(&script).output().unwrap();
        std::fs::write(&ready, "ready\n").unwrap();
        assert!(run().status.success());
        std::fs::write(&ready, "failed: provision/web.sh\n").unwrap();
        let failed = run();
        assert!(!failed.status.success());
        let said = String::from_utf8_lossy(&failed.stdout);
        assert!(
            said.contains("its set-up stopped (failed: provision/web.sh)") && said.contains("step two broke"),
            "{said}"
        );
        std::fs::write(&ready, "running: provision/web.sh\n").unwrap();
        let late = run();
        assert!(!late.status.success());
        assert!(String::from_utf8_lossy(&late.stdout).contains("not set up after 0 s (running: provision/web.sh)"));
    }
}
