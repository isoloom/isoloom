//! The steps a VM runs once, however often Vagrant provisions it. `vagrant provision` and `vagrant
//! up --provision` run every provisioner again, in order: Isoloom's own steps are written to
//! converge (running one again changes nothing), but a project's own script may not be, and a few
//! steps need the internet a machine offline (`internet: false`) no longer has once it is set up.
//! Such a step is guarded by a mark in the VM: done, it is skipped, unless what it runs changed (a
//! project script edited since, or the step itself). A step that fails leaves no mark, so the next
//! provisioning runs it again.

use isoloom_plan::lifecycle::Step;
use isoloom_render::stable_hash;
use isoloom_render::steps::PROJECT_DIR;
use isoloom_spec::shell::quote;

use super::transport::Os;

/// Where a Linux VM keeps the marks of its steps done.
pub(crate) const LINUX_DIR: &str = isoloom_spec::linux_state_path!("/done");
/// Where a Windows VM keeps them.
pub(crate) const WINDOWS_DIR: &str = isoloom_spec::windows_state_path!("\\done");

/// How a step runs once: the project file whose contents, when they change, run it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Once<'a> {
    /// The project script (from the project folder) it runs, if any.
    watched: Option<&'a str>,
}

impl<'a> Once<'a> {
    /// Whether `step` runs once on `os`, and how. A project script or playbook with no `creates`
    /// (its own guard), a shared volume (its NFS packages, installed from the internet), and the
    /// routes of a Windows VM (`route -p add` refuses a route it already has).
    pub(crate) fn of(step: &'a Step, os: Os) -> Option<Once<'a>> {
        match (os, step) {
            (_, Step::Run { creates: None, path, .. }) => Some(Once { watched: Some(path) }),
            (Os::Linux, Step::SharedVolume { .. }) | (Os::Windows, Step::Routes { .. }) => Some(Once { watched: None }),
            _ => None,
        }
    }

    /// `script`, the shell of a Linux VM's step `label`, the `at`th of its set-up, run once.
    pub(crate) fn linux(self, at: usize, label: &str, script: &str) -> String {
        let mark = format!("{LINUX_DIR}/{}", key(at, script));
        let sum = match self.watched {
            Some(path) => format!("$(cksum < {})", quote(&format!("{PROJECT_DIR}/{path}"))),
            None => "done".to_owned(),
        };
        let skipped = quote(&format!("{label}: done already (remove {mark} to run it again)"));
        format!(
            "isoloom_sum=\"{sum}\"\nif [ \"$(cat {mark} 2>/dev/null)\" = \"$isoloom_sum\" ]; then\n  echo {skipped}\nelse\n(\n{}\n) && mkdir -p {LINUX_DIR} && printf '%s\\n' \"$isoloom_sum\" > {mark}\nfi\n",
            script.trim_end_matches('\n')
        )
    }

    /// `script`, the PowerShell of a Windows VM's step `label`, the `at`th of its set-up, run
    /// once. A step that stops the script (`exit`, a `throw`) leaves no mark.
    pub(crate) fn windows(self, at: usize, label: &str, script: &str) -> String {
        let ps = isoloom_render::powershell::quote;
        let mark = ps(&format!("{WINDOWS_DIR}\\{}", key(at, script)));
        let sum = match self.watched {
            Some(path) => format!(
                "(Get-FileHash -LiteralPath {}).Hash",
                ps(&format!("{}\\{}", isoloom_spec::windows::ISOLOOM_DIR, path.replace('/', "\\")))
            ),
            None => "'done'".to_owned(),
        };
        let skipped = ps(&format!("{label}: done already (remove {WINDOWS_DIR}\\{} to run it again)", key(at, script)));
        format!(
            "$isoloomSum = {sum}; if ((Test-Path -LiteralPath {mark}) -and ((Get-Content -LiteralPath {mark} -Raw).Trim() -eq $isoloomSum)) {{ Write-Output {skipped} }} else {{ {}; New-Item -ItemType Directory -Force {} | Out-Null; Set-Content -LiteralPath {mark} $isoloomSum }}",
            script.trim_end_matches('\n'),
            ps(WINDOWS_DIR)
        )
    }
}

/// The name of the mark of the `at`th step, running `script`: a step changed, or moved, is
/// another step, run again.
fn key(at: usize, script: &str) -> String {
    format!("{at:02}-{:08x}", stable_hash::of(script))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str) -> Step {
        Step::Run {
            path: path.to_owned(),
            kind: isoloom_plan::lifecycle::RunKind::Shell,
            creates: None,
        }
    }

    #[test]
    fn a_script_with_creates_is_its_own_guard() {
        let guarded = Step::Run {
            path: "a.sh".into(),
            kind: isoloom_plan::lifecycle::RunKind::Shell,
            creates: Some("/done".into()),
        };
        assert_eq!(Once::of(&guarded, Os::Linux), None);
        assert_eq!(Once::of(&Step::Ready, Os::Linux), None);
        assert!(Once::of(&run("a.sh"), Os::Linux).is_some());
    }

    #[test]
    fn a_linux_step_runs_once_until_its_script_changes_and_again_after_it_failed() {
        if isoloom_render::test_support::posix_sh().is_none() {
            return;
        }
        let dir = tempfile::Builder::new().prefix("isoloom-once-").tempdir().expect("a temporary folder");
        let dir = dir.path();
        std::fs::write(dir.join("a.sh"), "echo one\n").expect("the script");
        let step = run("a.sh");
        let once = Once::of(&step, Os::Linux).expect("runs once");
        // The step counts its runs, and fails while `fail` exists.
        let body = "echo x >> runs\n[ ! -e fail ]\n";
        let script = once
            .linux(3, "a.sh", body)
            .replace(LINUX_DIR, "./done")
            .replace(&format!("{PROJECT_DIR}/a.sh"), "./a.sh");
        std::fs::write(dir.join("once.sh"), &script).expect("the guarded script");
        let provision = || {
            let mut posix = isoloom_render::test_support::posix_sh().expect("sh");
            posix.arg("once.sh").current_dir(dir).status().expect("sh runs").success()
        };
        let runs = || std::fs::read_to_string(dir.join("runs")).unwrap_or_default().lines().count();
        std::fs::write(dir.join("fail"), "").expect("the failure switch");
        assert!(!provision(), "a failed step fails the provisioner: {script}");
        std::fs::remove_file(dir.join("fail")).expect("the switch off");
        assert!(provision() && provision());
        assert_eq!(runs(), 2, "run again after its failure, then skipped once done");
        std::fs::write(dir.join("a.sh"), "echo two\n").expect("the script edited");
        assert!(provision() && provision());
        assert_eq!(runs(), 3, "an edited script runs again, once");
    }

    #[test]
    fn a_windows_step_marks_itself_done_after_its_script_only() {
        let step = run("dc.ps1");
        let ps = Once::of(&step, Os::Windows)
            .expect("runs once")
            .windows(0, "dc.ps1", "& run; if ($LASTEXITCODE) { exit $LASTEXITCODE }");
        let (before, after) = ps.split_once("& run;").expect("the step inside");
        assert!(
            before.contains("Get-FileHash -LiteralPath 'C:\\isoloom\\dc.ps1'") && before.contains("Test-Path"),
            "{ps}"
        );
        assert!(after.contains("Set-Content -LiteralPath 'C:\\ProgramData\\isoloom\\done\\00-"), "{ps}");
    }
}
