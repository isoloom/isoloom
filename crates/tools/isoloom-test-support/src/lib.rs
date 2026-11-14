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

/// Attribute `key` set directly in `body`, as text: a string's value, anything else (a
/// reference, a number, a call) as HCL writes it (`aws_network_interface.fw_dmz.id`, `true`).
///
/// # Panics
/// When `body` doesn't set it.
#[must_use]
pub fn hcl_text(body: &hcl::Body, key: &str) -> String {
    match hcl_attribute(body, key) {
        Some(hcl::Expression::String(s)) => s.clone(),
        Some(expr) => hcl::format::to_string(expr).unwrap_or_else(|e| panic!("`{key}` doesn't format: {e}")),
        None => panic!("no attribute `{key}` in {body:?}"),
    }
}

/// The blocks named `kind` directly in `body` (an instance's `network_interface`).
#[must_use]
pub fn hcl_nested<'a>(body: &'a hcl::Body, kind: &str) -> Vec<&'a hcl::Block> {
    body.blocks().filter(|b| b.identifier() == kind).collect()
}

/// The names of the `resource "<kind>" "<name>"` blocks of `tf` (`data` too, with `data`), in
/// order.
#[must_use]
pub fn hcl_names(body: &hcl::Body, block: &str, kind: &str) -> Vec<String> {
    body.blocks()
        .filter(|b| b.identifier() == block && b.labels().first().is_some_and(|l| l.as_str() == kind))
        .filter_map(|b| b.labels().get(1).map(|l| l.as_str().to_owned()))
        .collect()
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
        E::TemplateExpr(template) => out.push(template.to_string()),
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

/// A block of generated Ruby (a Vagrantfile): its opening line, its own statements and the
/// blocks inside it, read by structure. A here-document's lines belong to the statement that
/// opens it, so a script's text never reads as Ruby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RubyBlock {
    /// The opening line, trimmed (`config.vm.define "web" do |m|`); empty for the file.
    pub head: String,
    /// The statements directly in the block, in order: each line trimmed, a here-document's
    /// lines after its opening line, joined by line breaks.
    pub statements: Vec<String>,
    /// The blocks directly inside it, in order.
    pub blocks: Vec<RubyBlock>,
}

impl RubyBlock {
    /// The blocks anywhere inside this one whose opening line starts with `head`, outermost
    /// first.
    #[must_use]
    pub fn find(&self, head: &str) -> Vec<&RubyBlock> {
        let mut out = Vec::new();
        for b in &self.blocks {
            if b.head.starts_with(head) {
                out.push(b);
            }
            out.extend(b.find(head));
        }
        out
    }

    /// The one block anywhere inside this one whose opening line starts with `head`.
    ///
    /// # Panics
    /// When there isn't exactly one.
    #[must_use]
    pub fn block(&self, head: &str) -> &RubyBlock {
        match self.find(head).as_slice() {
            [one] => one,
            found => panic!("{} blocks open with `{head}` in `{}`", found.len(), self.head),
        }
    }

    /// The statements directly in this block that start with `prefix`.
    #[must_use]
    pub fn statements(&self, prefix: &str) -> Vec<&str> {
        self.statements.iter().map(String::as_str).filter(|s| s.starts_with(prefix)).collect()
    }

    /// Every statement in this block and the blocks inside it, the opening lines included: what
    /// a test asks of a whole block ("does the VM's define set this anywhere?").
    #[must_use]
    pub fn every_statement(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.statements.iter().map(String::as_str).collect();
        for b in &self.blocks {
            out.push(&b.head);
            out.extend(b.every_statement());
        }
        out
    }
}

/// A generated Ruby file (a Vagrantfile), read into its blocks: `do … end`, and `if`, `unless`,
/// `case`, `def`, `class`, `module`, `begin`, `while` and `until` opening a line, each closed by
/// its `end`. A string literal over several lines, and a here-document, belong to the statement
/// that opens them.
///
/// # Panics
/// When a block isn't closed, or an `end` closes none.
#[must_use]
pub fn ruby_blocks(text: &str) -> RubyBlock {
    let mut stack = vec![RubyBlock::default()];
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "end" || trimmed.starts_with("end.") || trimmed.starts_with("end ") || trimmed.starts_with("end)") {
            let done = stack.pop().unwrap_or_else(|| panic!("`end` closes no block:\n{text}"));
            let Some(parent) = stack.last_mut() else {
                panic!("`end` closes no block:\n{text}")
            };
            parent.blocks.push(done);
            continue;
        }
        let mut statement = trimmed.to_owned();
        let (mut open, mut tag) = scan(trimmed, None);
        while open.is_some() {
            let Some(next) = lines.next() else { break };
            statement.push('\n');
            statement.push_str(next.trim());
            let (still, more) = scan(next, open);
            open = still;
            tag = tag.or(more);
        }
        if let Some(tag) = tag {
            for body in lines.by_ref() {
                statement.push('\n');
                statement.push_str(body.trim());
                if body.trim() == tag {
                    break;
                }
            }
        }
        if opens_block(trimmed) {
            stack.push(RubyBlock {
                head: statement,
                ..RubyBlock::default()
            });
        } else if let Some(block) = stack.last_mut() {
            block.statements.push(statement);
        }
    }
    match stack.len() {
        1 => stack.pop().unwrap_or_default(),
        n => panic!("{} block(s) never closed:\n{text}", n - 1),
    }
}

/// Reads `line` from inside the string literal opened by `quote` (none: from code): the quote
/// still open at its end, and the terminator of the first here-document it opens outside a
/// string (`<<~'SH'`, `<<-EOS`, `<<RUBY`). A comment ends the code.
fn scan(line: &str, mut quote: Option<char>) -> (Option<char>, Option<String>) {
    let mut tag = None;
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match quote {
            Some(_) if c == '\\' => {
                chars.next();
            }
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => break,
            None if tag.is_none() && line.get(at..).is_some_and(|r| r.starts_with("<<")) => {
                let after = line.get(at + 2..).unwrap_or_default().trim_start_matches(['~', '-']);
                let quoted = after.starts_with(['\'', '"']);
                let name: String = after
                    .trim_start_matches(['\'', '"'])
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                    .collect();
                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    // Past the opener, its quoted tag included, so its quotes open no string.
                    let skip = (line.len() - at - after.len()) + name.len() + if quoted { 2 } else { 0 } - 1;
                    for _ in 0..skip {
                        chars.next();
                    }
                    tag = Some(name);
                }
            }
            None => {}
        }
    }
    (quote, tag)
}

/// Whether `line` (trimmed, a comment's text left out) opens a block its own `end` closes.
fn opens_block(line: &str) -> bool {
    let code = line.split(" # ").next().unwrap_or(line).trim_end();
    let keyword = ["if ", "unless ", "case ", "def ", "class ", "module ", "while ", "until "]
        .iter()
        .any(|k| code.starts_with(k))
        || code == "begin"
        || code.ends_with(" begin");
    let do_block = code.ends_with(" do") || (code.contains(" do |") && code.ends_with('|'));
    (keyword || do_block) && !code.ends_with(" end") && !code.contains("; end")
}

/// A rendered YAML file (Compose, Kubernetes, Ansible), parsed.
///
/// # Panics
/// When it isn't YAML.
#[must_use]
pub fn yaml(text: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("not YAML: {e}\n{text}"))
}

/// The value at `path` in `value`: keys separated by `.`, a list's item by its index
/// (`services.web.networks.lan.ipv4_address`, `services.web.ports.0`); a key holding a `.` is
/// written in backquotes (`` labels.`isoloom.managed` ``).
#[must_use]
pub fn yaml_at<'v>(value: &'v serde_yaml_ng::Value, path: &str) -> Option<&'v serde_yaml_ng::Value> {
    let mut at = value;
    for key in yaml_keys(path) {
        at = match at {
            serde_yaml_ng::Value::Sequence(items) => items.get(key.parse::<usize>().ok()?)?,
            serde_yaml_ng::Value::Mapping(map) => map.get(key.as_str())?,
            _ => return None,
        };
    }
    Some(at)
}

/// The text of the scalar at `path` in `value` (a string, a number or a boolean as written).
///
/// # Panics
/// When there is none there, or it isn't a scalar.
#[must_use]
pub fn yaml_str(value: &serde_yaml_ng::Value, path: &str) -> String {
    match yaml_at(value, path) {
        Some(serde_yaml_ng::Value::String(s)) => s.clone(),
        Some(serde_yaml_ng::Value::Number(n)) => n.to_string(),
        Some(serde_yaml_ng::Value::Bool(b)) => b.to_string(),
        other => panic!("no scalar at `{path}` (found {other:?})"),
    }
}

/// The texts of the list at `path` in `value`, each item a scalar; empty when there is no list.
#[must_use]
pub fn yaml_strs(value: &serde_yaml_ng::Value, path: &str) -> Vec<String> {
    match yaml_at(value, path) {
        Some(serde_yaml_ng::Value::Sequence(items)) => items
            .iter()
            .map(|v| match v {
                serde_yaml_ng::Value::String(s) => s.clone(),
                other => serde_yaml_ng::to_string(other).unwrap_or_default().trim_end().to_owned(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The keys of the mapping at `path` in `value`, in order; empty when there is none.
#[must_use]
pub fn yaml_keys_at(value: &serde_yaml_ng::Value, path: &str) -> Vec<String> {
    let at = if path.is_empty() { Some(value) } else { yaml_at(value, path) };
    match at {
        Some(serde_yaml_ng::Value::Mapping(map)) => map.keys().filter_map(|k| k.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// `path`'s keys: split at `.`, a backquoted key whole.
fn yaml_keys(path: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut key = String::new();
    let mut quoted = false;
    for c in path.chars() {
        match c {
            '`' => quoted = !quoted,
            '.' if !quoted => keys.push(std::mem::take(&mut key)),
            c => key.push(c),
        }
    }
    keys.push(key);
    keys
}

/// Every document of a rendered multi-document YAML file (Kubernetes manifests), parsed; the
/// empty ones left out.
///
/// # Panics
/// When a document isn't YAML.
#[must_use]
pub fn yaml_documents(text: &str) -> Vec<serde_yaml_ng::Value> {
    use serde::Deserialize as _;
    serde_yaml_ng::Deserializer::from_str(text)
        .map(|d| serde_yaml_ng::Value::deserialize(d).unwrap_or_else(|e| panic!("not YAML: {e}\n{text}")))
        .filter(|v| !v.is_null())
        .collect()
}

/// The one manifest of `kind` named `name` among `docs`.
///
/// # Panics
/// When there isn't exactly one.
#[must_use]
pub fn manifest<'d>(docs: &'d [serde_yaml_ng::Value], kind: &str, name: &str) -> &'d serde_yaml_ng::Value {
    let found: Vec<_> = docs
        .iter()
        .filter(|d| d.get("kind").and_then(serde_yaml_ng::Value::as_str) == Some(kind))
        .filter(|d| yaml_at(d, "metadata.name").and_then(serde_yaml_ng::Value::as_str) == Some(name))
        .collect();
    match found.as_slice() {
        [one] => one,
        _ => panic!("{} {kind} `{name}` among the manifests", found.len()),
    }
}

/// The names of the manifests of `kind` among `docs`, in order.
#[must_use]
pub fn manifest_names(docs: &[serde_yaml_ng::Value], kind: &str) -> Vec<String> {
    docs.iter()
        .filter(|d| d.get("kind").and_then(serde_yaml_ng::Value::as_str) == Some(kind))
        .filter_map(|d| yaml_at(d, "metadata.name").and_then(serde_yaml_ng::Value::as_str).map(str::to_owned))
        .collect()
}

/// The item of the list at `path` in `value` whose `name` is `name` (a pod's container, a
/// volume, an init container).
///
/// # Panics
/// When there isn't exactly one.
#[must_use]
pub fn named<'v>(value: &'v serde_yaml_ng::Value, path: &str, name: &str) -> &'v serde_yaml_ng::Value {
    let items = yaml_at(value, path)
        .and_then(serde_yaml_ng::Value::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let found: Vec<_> = items
        .iter()
        .filter(|i| i.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(name))
        .collect();
    match found.as_slice() {
        [one] => one,
        _ => panic!("{} items named `{name}` at `{path}`", found.len()),
    }
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
        let _ = ruby_blocks("config.vm.define \"web\" do |m|\n  m.vm.box = \"x\"\n");
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
