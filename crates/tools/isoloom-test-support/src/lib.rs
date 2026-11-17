//! What the Isoloom crates' tests share:
//!
//! - [`FakeBin`]: stand-in tools on a `PATH` holding nothing else, for running a generated shell
//!   script; a tool neither stood in nor linked is missing, so a call the test didn't script fails
//!   the script, and the calls it made are read back as argument lists.
//! - [`ChildGuard`]: a helper process killed with its process group and reaped when dropped, so
//!   none outlives its test (nor holds a build's lock).
//! - [`write_executable`]: a program a test runs, written by a child process. A test never
//!   executes a file its own process wrote: that file is open for writing in every process
//!   another test thread forks meanwhile, and executing it then fails with "Text file busy".
//! - [`assert_every!`]: fails on an empty collection, where `iter().all(..)` passes.
//! - [`needs`]: a test of a real tool fails naming the tool when it isn't there; [`tool`] starts
//!   it.
//! - [`hcl_body`], [`yaml`], [`ruby_blocks`]: a rendered file parsed, so a test asserts on its values rather than on
//!   substrings of its text.
//! - [`sh_decode`], [`hcl_decode`] and the other decoders: a literal the builders write read back
//!   as its language reads it, for the quoting properties and the fuzz target.
//!
//! A test-only crate: it panics where a test should fail.

#![expect(clippy::panic, reason = "a test-support helper fails the test that called it")]
#![expect(
    clippy::disallowed_methods,
    reason = "the tests' helpers run the tools a test checks generated files with: this crate is their adapter"
)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};

/// A folder of stand-in tools, the whole `PATH` of the scripts it runs.
#[derive(Debug)]
pub struct FakeBin {
    dir: tempfile::TempDir,
}

/// What a fallible test step gives.
pub type Outcome<T> = Result<T, Box<dyn std::error::Error>>;

impl FakeBin {
    /// An empty one.
    ///
    /// # Errors
    /// When its folder can't be made.
    pub fn new() -> Outcome<FakeBin> {
        FakeBin::named("fakebin")
    }

    /// An empty one whose folder's name starts with `name` (what a test left is found by it).
    ///
    /// # Errors
    /// When its folder can't be made.
    pub fn named(name: &str) -> Outcome<FakeBin> {
        let dir = tempfile::Builder::new().prefix(&format!("isoloom-{name}-")).tempdir()?;
        std::fs::create_dir_all(dir.path().join("bin"))?;
        Ok(FakeBin { dir })
    }

    /// Its scratch folder (the stand-ins are in `bin/` under it).
    #[must_use]
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The `PATH` its scripts get: its `bin/` alone.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    /// The log the [`FakeBin::recording`] stand-ins append to.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.dir.path().join("calls.log")
    }

    /// A stand-in `name` running the shell `body`.
    ///
    /// # Errors
    /// When it can't be written.
    pub fn stand_in(&self, name: &str, body: &str) -> Outcome<&FakeBin> {
        write_executable(&self.path().join(name), &format!("#!/bin/sh\n{body}\n"), true)?;
        Ok(self)
    }

    /// A stand-in `name` that records each call (its arguments, one call a line) and succeeds.
    ///
    /// # Errors
    /// When it can't be written.
    pub fn recording(&self, name: &str) -> Outcome<&FakeBin> {
        self.stand_in(name, &format!("echo \"{name} $*\" >> '{}'", self.log().display()))
    }

    /// The host's own `tools` (text tools a script uses: `awk`, `grep`...), linked in.
    ///
    /// # Errors
    /// When one isn't found on the host's `PATH`.
    pub fn link_real(&self, tools: &[&str]) -> Outcome<&FakeBin> {
        for tool in tools {
            let found = Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output()?;
            let found = String::from_utf8(found.stdout)?;
            if found.trim().is_empty() {
                return Err(format!("`{tool}` isn't on this host's PATH").into());
            }
            link(Path::new(found.trim()), &self.path().join(tool))?;
        }
        Ok(self)
    }

    /// `script` run by `/bin/sh` with nothing of the test's environment but `envs`, its `PATH`
    /// this folder's tools: what it did.
    ///
    /// # Errors
    /// When it can't be started.
    pub fn run(&self, script: &str, envs: &[(&str, &Path)]) -> Outcome<Output> {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script).env_clear().env("PATH", self.path()).env("LC_ALL", "C");
        for (name, value) in envs {
            command.env(name, value);
        }
        Ok(command.output()?)
    }

    /// [`FakeBin::run`], which must succeed: the calls the recording stand-ins saw.
    ///
    /// # Errors
    /// When it can't be started or the log can't be read.
    ///
    /// # Panics
    /// When the script fails (a tool it called but the test didn't provide included).
    pub fn calls_of(&self, script: &str, envs: &[(&str, &Path)]) -> Outcome<Vec<Vec<String>>> {
        let out = self.run(script, envs)?;
        assert!(out.status.success(), "the script failed: {}", String::from_utf8_lossy(&out.stderr));
        self.calls()
    }

    /// The calls the recording stand-ins saw so far, each as its program and arguments.
    ///
    /// # Errors
    /// When the log is there but can't be read.
    pub fn calls(&self) -> Outcome<Vec<Vec<String>>> {
        let text = match std::fs::read_to_string(self.log()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(text.lines().map(|l| l.split_whitespace().map(str::to_owned).collect()).collect())
    }
}

/// Writes `contents` to `path`, executable or not, from a child process (`cat`) rather than from
/// the test's own: a file this process has open for writing is open in every process another
/// test thread forks meanwhile, until that one execs, and executing the file in that window fails
/// with "Text file busy" (ETXTBSY), so a test that writes a program and runs it failed at random
/// when the tests beside it started processes.
///
/// # Errors
/// When it can't be written.
#[cfg(unix)]
pub fn write_executable(path: &Path, contents: &str, executable: bool) -> std::io::Result<()> {
    use std::io::Write;
    let mut cat = Command::new("sh")
        .arg("-c")
        .arg("cat > \"$0\" && chmod \"$1\" \"$0\"")
        .arg(path)
        .arg(if executable { "755" } else { "644" })
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    cat.stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no stdin"))?
        .write_all(contents.as_bytes())?;
    let status = cat.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("writing {}: {status}", path.display())))
    }
}

/// [`write_executable`] where there is no exec race to avoid.
///
/// # Errors
/// When it can't be written.
#[cfg(not(unix))]
pub fn write_executable(path: &Path, contents: &str, _executable: bool) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

#[cfg(not(unix))]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to).map(drop)
}

/// A spawned helper process, killed with its process group and reaped when dropped.
#[derive(Debug)]
pub struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    /// Spawns `command` in a process group of its own (on Unix), so what it starts goes with it.
    ///
    /// # Errors
    /// When it can't be spawned.
    pub fn spawn(command: &mut Command) -> std::io::Result<ChildGuard> {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(command, 0);
        Ok(ChildGuard { child: command.spawn()? })
    }

    /// Its process id.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // The whole group: what the helper started goes with it.
            Command::new("kill").args(["-KILL", "--", &format!("-{}", self.child.id())]).output().ok();
        }
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// Fails naming the first of `tools` that isn't on the `PATH`: a test that needs a real tool is
/// ignored by default (a player's machine lacks it) and `cargo xtask test` runs it, so a missing
/// tool says so rather than fail on its first call.
///
/// # Errors
/// The first tool not found.
pub fn needs(tools: &[&str]) -> Outcome<()> {
    for tool in tools {
        // Looked up, not started: a tool that starts leaves its own files (PowerShell's pipe in
        // the temporary folder) for the test run's leftover guard to find.
        let found = std::env::var_os("PATH")
            .is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(format!("{tool}{}", std::env::consts::EXE_SUFFIX)).is_file()));
        if !found {
            return Err(format!("needs `{tool}` on PATH").into());
        }
    }
    Ok(())
}

/// A real tool a test runs on what the crate writes (`sh` on a generated script, `python3` as
/// a second implementation), to set up and start: this crate is the tests' adapter, so their
/// own code starts no process.
#[must_use]
pub fn tool(program: &str) -> Command {
    Command::new(program)
}

/// Asserts that `$pred` holds for every item of `$items`, naming the first that doesn't, and that
/// there is at least one item.
#[macro_export]
macro_rules! assert_every {
    ($items:expr, $pred:expr $(,)?) => {{
        let items: Vec<_> = ::std::iter::IntoIterator::into_iter($items).collect();
        assert!(!items.is_empty(), "nothing to check: `{}` is empty", stringify!($items));
        let pred = $pred;
        for item in &items {
            assert!(pred(item), "`{}` doesn't hold for {:?}", stringify!($pred), item);
        }
    }};
}

mod hcl_template;
mod literals;
mod parsed_hcl;
mod parsed_ini;
mod parsed_ruby;
mod parsed_yaml;

/// The HCL crate the readers parse with, for tests that name its types.
pub use hcl;
pub use hcl_template::{HclPiece, hcl_decode, hcl_pieces};
pub use literals::{b64_decode, ps_decode, rb_decode, sh_decode, systemd_decode};
pub use parsed_hcl::{hcl_attribute, hcl_blocks, hcl_body, hcl_literals, hcl_map, hcl_names, hcl_nested, hcl_one, hcl_resource, hcl_text, hcl_values};
pub use parsed_ini::ini_groups;
pub use parsed_ruby::{RubyBlock, ruby_blocks};
pub use parsed_yaml::{manifest, manifest_names, named, yaml, yaml_at, yaml_documents, yaml_keys_at, yaml_str, yaml_strs, yaml_values};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "nothing to check")]
    fn assert_every_fails_on_nothing() {
        assert_every!(Vec::<u8>::new(), |_: &u8| true);
    }

    #[test]
    #[should_panic(expected = "doesn't hold for -2")]
    fn assert_every_names_the_first_item_that_fails() {
        assert_every!([1, -2, 3], |n: &i32| *n > 0);
    }

    #[test]
    fn assert_every_passes_when_each_item_holds() {
        assert_every!([1, 2, 3], |n: &i32| *n > 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_script_sees_only_the_tools_given_it_and_its_calls_are_read_back() -> Outcome<()> {
        let bin = FakeBin::new()?;
        bin.recording("systemctl")?;
        let calls = bin.calls_of("systemctl enable --now qemu-guest-agent", &[])?;
        assert_eq!(calls, [["systemctl", "enable", "--now", "qemu-guest-agent"]]);
        let missing = bin.run("ip route show", &[])?;
        assert_eq!(missing.status.code(), Some(127), "a tool not given is missing");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_stand_in_runs_its_body_with_the_arguments_and_only_the_environment_given() -> Outcome<()> {
        let bin = FakeBin::named("stand-in")?;
        let name = bin.dir().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        assert!(name.starts_with("isoloom-stand-in-"), "a leftover is found by its name: {name}");
        bin.stand_in("greet", "echo \"$1 from $AT, home: ${HOME:-none}\"")?;
        let out = bin.run("greet hello", &[("AT", Path::new("/srv"))])?;
        assert_eq!(String::from_utf8(out.stdout)?, "hello from /srv, home: none\n");
        assert_eq!(bin.calls()?, Vec::<Vec<String>>::new(), "nothing recorded yet");
        assert_eq!(bin.log(), bin.dir().join("calls.log"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_real_tool_is_linked_in_and_one_the_host_lacks_is_an_error_naming_it() -> Outcome<()> {
        let bin = FakeBin::new()?;
        bin.link_real(&["tr"])?;
        let out = bin.run("echo abc | tr a-z A-Z", &[])?;
        assert_eq!(String::from_utf8(out.stdout)?, "ABC\n");
        let missing = bin.link_real(&["isoloom-no-such-tool"]).err().map(|e| e.to_string());
        assert_eq!(missing.as_deref(), Some("`isoloom-no-such-tool` isn't on this host's PATH"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    #[should_panic(expected = "the script failed: boom")]
    fn the_calls_of_a_script_that_failed_fail_the_test() {
        let bin = FakeBin::new().unwrap();
        let _calls = bin.calls_of("echo boom >&2; exit 1", &[]);
    }

    #[cfg(unix)]
    #[test]
    fn a_program_is_written_with_its_mode_and_runs() -> Outcome<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir()?;
        let (program, data) = (dir.path().join("hello"), dir.path().join("data"));
        write_executable(&program, "#!/bin/sh\necho hi\n", true)?;
        write_executable(&data, "x", false)?;
        let mode = |p: &Path| std::fs::metadata(p).map(|m| m.permissions().mode() & 0o777);
        assert_eq!((mode(&program)?, mode(&data)?), (0o755, 0o644));
        assert_eq!(Command::new(&program).output()?.stdout, b"hi\n");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_guarded_helper_and_what_it_started_end_with_the_guard() -> Outcome<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("pid");
        let script = format!("sleep 300 & echo $! > '{}'; wait", file.display());
        let guard = ChildGuard::spawn(Command::new("sh").args(["-c", &script]))?;
        assert!(guard.id() > 0);
        let mut pid = None;
        for _ in 0..500 {
            pid = std::fs::read_to_string(&file).ok().and_then(|p| p.trim().parse::<u32>().ok());
            if pid.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pid = pid.ok_or("the helper never started")?;
        drop(guard);
        let gone = (0..500).any(|_| {
            let alive = Command::new("kill").args(["-0", &pid.to_string()]).status().is_ok_and(|s| s.success());
            if alive {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            !alive
        });
        assert!(gone, "the helper's child {pid} outlived the guard");
        Ok(())
    }

    #[test]
    fn a_tool_on_the_path_is_there_and_a_missing_one_is_named() {
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(needs(&[shell]).is_ok());
        let missing = needs(&[shell, "isoloom-no-such-tool"]).err().map(|e| e.to_string());
        assert_eq!(missing.as_deref(), Some("needs `isoloom-no-such-tool` on PATH"));
    }
}
