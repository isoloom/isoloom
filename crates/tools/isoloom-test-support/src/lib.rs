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
//! - [`hcl_body`], [`yaml`]: a rendered file parsed, so a test asserts on its values rather than on
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

/// A rendered HCL file (Terraform), parsed.
///
/// # Panics
/// When it isn't HCL.
#[must_use]
pub fn hcl_body(text: &str) -> hcl::Body {
    hcl::parse(text).unwrap_or_else(|e| panic!("not HCL: {e}\n{text}"))
}

/// The blocks of `body` of type `kind` with labels `labels` (`resource`, `["aws_instance", "web"]`).
#[must_use]
pub fn hcl_blocks<'a>(body: &'a hcl::Body, kind: &str, labels: &[&str]) -> Vec<&'a hcl::Block> {
    body.blocks()
        .filter(|b| b.identifier() == kind && b.labels().iter().map(hcl::BlockLabel::as_str).zip(labels).all(|(got, want)| got == *want))
        .collect()
}

/// Every literal value (number, string, boolean) anywhere in `body`, its nested blocks, lists,
/// objects, function arguments and conditionals included, as text: what a test asks of a block
/// ("does it open port 18080?") without matching digits inside a CIDR, a size or a name.
#[must_use]
pub fn hcl_literals(body: &hcl::Body) -> Vec<String> {
    let mut out = Vec::new();
    for structure in body {
        match structure {
            hcl::Structure::Attribute(a) => literals_of(a.expr(), &mut out),
            hcl::Structure::Block(b) => out.extend(hcl_literals(b.body())),
        }
    }
    out
}

/// The literal values of every attribute `key` anywhere in `body`, its nested blocks included
/// (`volume_size` in an instance's `root_block_device`): an attribute read by name, never by a
/// line that holds `key =`.
#[must_use]
pub fn hcl_values(body: &hcl::Body, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for structure in body {
        match structure {
            hcl::Structure::Attribute(a) if a.key() == key => literals_of(a.expr(), &mut out),
            hcl::Structure::Attribute(_) => {}
            hcl::Structure::Block(b) => out.extend(hcl_values(b.body(), key)),
        }
    }
    out
}

/// The expression of attribute `key` set directly in `body` (not in its nested blocks).
#[must_use]
pub fn hcl_attribute<'a>(body: &'a hcl::Body, key: &str) -> Option<&'a hcl::Expression> {
    body.attributes().find(|a| a.key() == key).map(hcl::Attribute::expr)
}

/// The body of the one `resource "<kind>" "<name>"` of `tf`, parsed.
///
/// # Panics
/// When `tf` isn't HCL or doesn't have exactly one such resource.
#[must_use]
pub fn hcl_resource(tf: &str, kind: &str, name: &str) -> hcl::Body {
    let body = hcl_body(tf);
    let found = hcl_blocks(&body, "resource", &[kind, name]);
    match found.as_slice() {
        [one] => one.body().clone(),
        _ => panic!("{} `resource \"{kind}\" \"{name}\"` in:\n{tf}", found.len()),
    }
}

fn literals_of(expr: &hcl::Expression, out: &mut Vec<String>) {
    use hcl::Expression as E;
    match expr {
        E::Number(n) => out.push(n.to_string()),
        E::String(s) => out.push(s.clone()),
        E::Bool(b) => out.push(b.to_string()),
        E::Array(items) => items.iter().for_each(|e| literals_of(e, out)),
        E::Object(object) => object.values().for_each(|e| literals_of(e, out)),
        E::Parenthesis(inner) => literals_of(inner, out),
        E::FuncCall(call) => call.args.iter().for_each(|e| literals_of(e, out)),
        E::Conditional(c) => {
            literals_of(&c.true_expr, out);
            literals_of(&c.false_expr, out);
        }
        _ => {}
    }
}

/// A rendered YAML file (Compose, Kubernetes, Ansible), parsed.
///
/// # Panics
/// When it isn't YAML.
#[must_use]
pub fn yaml(text: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("not YAML: {e}\n{text}"))
}

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
