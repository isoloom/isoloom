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
//! - [`hcl_body`], [`yaml`], [`ruby_blocks`]: a rendered file parsed, so a test asserts on its values rather than on
//!   substrings of its text.
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

mod parsed_hcl;
mod parsed_ruby;
mod parsed_yaml;

/// The HCL crate the readers parse with, for tests that name its types.
pub use hcl;
pub use parsed_hcl::{hcl_attribute, hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_resource, hcl_text, hcl_values};
pub use parsed_ruby::{RubyBlock, ruby_blocks};
pub use parsed_yaml::{manifest, manifest_names, named, yaml, yaml_at, yaml_documents, yaml_keys_at, yaml_str, yaml_strs};

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
    fn a_guarded_helper_and_what_it_started_end_with_the_guard() -> Outcome<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("pid");
        let script = format!("sleep 300 & echo $! > '{}'; wait", file.display());
        let guard = ChildGuard::spawn(Command::new("sh").args(["-c", &script]))?;
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
    fn a_yaml_value_is_read_by_its_path() {
        let v = yaml("services:\n  web:\n    labels: { isoloom.managed: 'true' }\n    ports: ['8080:80']\n    networks: { lan: { ipv4_address: 10.0.0.2 } }\n");
        assert_eq!(yaml_str(&v, "services.web.networks.lan.ipv4_address"), "10.0.0.2");
        assert_eq!(yaml_str(&v, "services.web.labels.`isoloom.managed`"), "true");
        assert_eq!(yaml_strs(&v, "services.web.ports"), ["8080:80"]);
        assert_eq!(yaml_str(&v, "services.web.ports.0"), "8080:80");
        assert_eq!(yaml_keys_at(&v, "services"), ["web"]);
        assert!(yaml_at(&v, "services.db").is_none());
    }

    #[test]
    fn a_vagrantfile_is_read_by_block_its_heredocs_left_whole() {
        let vf = "Vagrant.configure(\"2\") do |config|\n  config.vm.define \"web\" do |m|\n    m.vm.box = \"bento/debian-12\"\n    m.vm.provision \"shell\", inline: \"cat > x <<'EOF'\\nEOF\\n\"\n    m.vm.provision \"shell\", inline: <<~'SH'\n      if true; then\n      echo end\n      SH\n    if ENV[\"X\"]\n      m.vm.box = \"other\"\n    end\n  end\nend\n";
        let file = ruby_blocks(vf);
        let web = file.block("config.vm.define \"web\"");
        assert_eq!(web.statements("m.vm.box"), ["m.vm.box = \"bento/debian-12\""]);
        let provision = web.statements("m.vm.provision");
        assert_eq!(provision.len(), 2, "a `<<` in a string opens no here-document: {provision:?}");
        assert_eq!(provision[1], "m.vm.provision \"shell\", inline: <<~'SH'\nif true; then\necho end\nSH");
        assert_eq!(web.block("if ENV").statements, ["m.vm.box = \"other\""]);
        assert!(web.every_statement().contains(&"m.vm.box = \"other\""));
    }

    #[test]
    #[should_panic(expected = "never closed")]
    fn an_unclosed_ruby_block_fails() {
        let read = ruby_blocks("config.vm.define \"web\" do |m|\n  m.vm.box = \"x\"\n");
        assert!(read.blocks.is_empty(), "{read:?}");
    }

    #[test]
    fn a_rendered_hcl_file_is_read_by_block_and_attribute() {
        let body = hcl_body("resource \"aws_instance\" \"web\" {\n  instance_type = \"t3.micro\"\n}\nresource \"aws_vpc\" \"lab\" {}\n");
        let web = hcl_blocks(&body, "resource", &["aws_instance", "web"]);
        assert_eq!(web.len(), 1);
        let kind = web
            .first()
            .and_then(|b| b.body().attributes().find(|a| a.key() == "instance_type"))
            .map(|a| a.expr().to_string());
        assert_eq!(kind.as_deref(), Some("\"t3.micro\""));
    }
}
