//! Running the tools the tasks drive: the cargo running this xtask, a command attached to the
//! terminal, and what to say when one fails.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::sync::OnceLock;

use crate::Res;

// The command describers and captured-output runners are shared with Isothrum through the ratchet
// crate; the rest of the runners live here.
pub(crate) use xtask_quality::process::{describe, output, quiet, without_run_env};

/// `cargo`, the one running this xtask (`$CARGO`, so the same toolchain), in `dir`, without the
/// variables `cargo run` set for the xtask, compiling through sccache when it is installed
/// ([`compiler_cache`]).
pub(crate) fn cargo(dir: &Path) -> Command {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    without_run_env(&mut cmd);
    compiler_cache(&mut cmd);
    cmd.current_dir(dir);
    cmd
}

/// sccache as `cmd`'s compiler wrapper (`RUSTC_WRAPPER`) when it is installed and no wrapper is
/// set (an empty `RUSTC_WRAPPER` says none): a crate another worktree or target folder already
/// compiled comes from its cache. Never required: without it, `cmd` is left as it is.
pub(crate) fn compiler_cache(cmd: &mut Command) -> &mut Command {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    let installed = *INSTALLED.get_or_init(|| first_line(Command::new("sccache").arg("--version")).is_some());
    if installed && std::env::var_os("RUSTC_WRAPPER").is_none() {
        cmd.env("RUSTC_WRAPPER", "sccache");
    }
    cmd
}

/// Runs a command attached to the terminal; whether it succeeded. An error only when it can't
/// start.
pub(crate) fn passed(cmd: &mut Command) -> Res<bool> {
    let status = cmd.status().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    Ok(status.success())
}

/// The exit code of a task whose steps all had to pass.
pub(crate) fn exit_code(ok: bool) -> ExitCode {
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Runs a command attached to the terminal; an error names it when it fails.
pub(crate) fn run(cmd: &mut Command) -> Res<()> {
    let status = cmd.status().map_err(|e| format!("`{}` couldn't start: {e}", describe(cmd)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{}` failed ({status})", describe(cmd)).into())
    }
}

/// The first line a command prints on success, or `None` (not installed, or it failed).
pub(crate) fn first_line(cmd: &mut Command) -> Option<String> {
    let out = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| text.lines().next().unwrap_or("").trim().to_owned())
}

/// `program`, started by its name on the `PATH`: the tools the tasks drive (`docker`, `curl`). In
/// this crate's tests, the stand-in a test put in its place ([`testing::with_stand_ins`]).
pub(crate) fn tool(program: &str) -> Command {
    #[cfg(test)]
    if let Some(stand_in) = testing::stand_in(program) {
        return Command::new(stand_in);
    }
    Command::new(program)
}

/// The tests' seam: the tools [`tool`] starts, taken from a folder of stand-ins.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        /// The folder the tools come from on this thread, while a test says so.
        static STAND_INS: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    /// `f` run with every tool [`super::tool`] starts on this thread taken from `bin`: its
    /// stand-in there, else a tool that isn't installed. A panic in `f` puts the real tools back.
    #[cfg(unix)]
    pub(crate) fn with_stand_ins<T>(bin: &std::path::Path, f: impl FnOnce() -> T) -> T {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                STAND_INS.with(|s| s.borrow_mut().take());
            }
        }
        STAND_INS.with(|s| *s.borrow_mut() = Some(bin.to_path_buf()));
        let _restore = Restore;
        f()
    }

    /// Where `program` comes from while [`with_stand_ins`] runs.
    pub(super) fn stand_in(program: &str) -> Option<PathBuf> {
        STAND_INS.with(|s| s.borrow().as_ref().map(|bin| bin.join(program)))
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use isoloom_test_support::FakeBin;

    use super::*;

    /// A folder with `ok` (prints two lines, succeeds) and `no` (fails).
    fn tools() -> FakeBin {
        let bin = FakeBin::named("xtask-process").unwrap();
        bin.stand_in("ok", "echo ' first '; echo second")
            .unwrap()
            .stand_in("no", "echo said; exit 3")
            .unwrap();
        bin
    }

    #[test]
    fn a_tool_that_failed_is_named_with_its_status_and_one_that_ran_passes() {
        let bin = tools();
        testing::with_stand_ins(&bin.path(), || {
            assert_eq!(passed(tool("ok").stdout(std::process::Stdio::null())).ok(), Some(true));
            assert_eq!(passed(tool("no").stdout(std::process::Stdio::null())).ok(), Some(false));
            assert!(run(tool("ok").stdout(std::process::Stdio::null())).is_ok());
            let failed = run(tool("no").arg("x").stdout(std::process::Stdio::null())).unwrap_err().to_string();
            assert_eq!(failed, format!("`{} x` failed (exit status: 3)", bin.path().join("no").display()));
        });
    }

    #[test]
    fn a_tool_that_is_not_installed_is_an_error_naming_it_not_a_failure() {
        let bin = tools();
        testing::with_stand_ins(&bin.path(), || {
            let missing = bin.path().join("docker");
            for e in [passed(&mut tool("docker")).unwrap_err(), run(&mut tool("docker")).unwrap_err()] {
                let e = e.to_string();
                assert!(e.starts_with(&format!("`{}` couldn't start: ", missing.display())), "{e}");
            }
        });
    }

    #[test]
    fn the_first_line_is_a_successful_tools_trimmed_and_none_otherwise() {
        let bin = tools();
        testing::with_stand_ins(&bin.path(), || {
            assert_eq!(first_line(&mut tool("ok")).as_deref(), Some("first"));
            assert_eq!(first_line(&mut tool("no")), None, "it failed");
            assert_eq!(first_line(&mut tool("docker")), None, "not installed");
        });
    }

    #[test]
    fn outside_a_test_s_stand_ins_a_tool_is_its_name_on_the_path() {
        assert_eq!(tool("docker").get_program(), "docker");
        let bin = tools();
        testing::with_stand_ins(&bin.path(), || assert_eq!(tool("docker").get_program(), bin.path().join("docker")));
        assert_eq!(tool("docker").get_program(), "docker", "put back after");
    }
}
