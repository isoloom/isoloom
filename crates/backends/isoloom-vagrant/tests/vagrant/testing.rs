//! What the Vagrant backend's integration tests share: the examples planned and rendered, a
//! spec planned, the generated Ruby loaded on a stand-in for Vagrant, and its calls' keywords.

use std::error::Error;
use std::path::Path;
use std::process::Command;

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_vagrant::VagrantBackend;

/// A test's outcome: a failed step is its report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// An example of the project, which must be valid, planned for Vagrant.
pub(crate) fn plan(example: &str) -> Outcome<Environment> {
    let lab = isoloom_plan::Lab::new(isoloom_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    )?);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("{example} isn't valid: {problems:?}").into());
    }
    Ok(lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
}

/// The files the Vagrant backend renders for an example, from its plan.
pub(crate) fn render(example: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(VagrantBackend.render(&plan(example)?)?)
}

/// An example's Vagrantfile.
pub(crate) fn vagrantfile(example: &str) -> Outcome<String> {
    let file = render(example)?
        .into_iter()
        .find(|f| f.path.ends_with("/Vagrantfile"))
        .ok_or("no Vagrantfile")?;
    Ok(file.contents)
}

/// A spec written in a test, planned for Vagrant.
pub(crate) fn parsed(yaml: &str) -> Outcome<Environment> {
    let lab = isoloom_plan::Lab::parse(yaml).map_err(|problems| format!("{problems:?}"))?;
    Ok(lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
}

/// A stand-in for Vagrant that records what a generated Vagrantfile (`ARGV[0]`) says, then loads
/// it: each VM's forwarded ports (`$forwards`) and typed action triggers (`$triggers`, by VM, the
/// action's name and when). Vagrant's own Ruby is 3.x; macOS's `ruby` is 2.6, which lacks
/// `filter_map`, so it gets one.
pub(crate) const VAGRANT_STAND_IN_RB: &str = r#"
Enumerable.module_eval { def filter_map; each_with_object([]) { |x, out| v = yield(x); out << v if v }; end } unless Enumerable.method_defined?(:filter_map)
$forwards = []
$triggers = {}
$ps = []
class Rec
  def initialize(path = []); @path = path; end
  def respond_to_missing?(*); true; end
  def method_missing(name, *args, &blk)
    $machine = args[0] if name == :define
    if name == :network && args[0] == "forwarded_port"
      $forwards << [$machine, args[1][:guest], args[1][:host]]
    end
    if @path.last == :trigger
      t = Trigger.new
      blk.call(t)
      $triggers[[$machine, args[0].to_s.split("::").last, name]] = t.code
      return nil
    end
    blk.call(*Array.new([blk.arity, 1].max) { Rec.new }) if blk
    Rec.new(@path + [name])
  end
end
class Trigger
  attr_reader :code
  def ruby(&blk); @code = blk; end
end
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); yield Rec.new; end
  def self.user_data_path; "/nonexistent"; end
end
load ARGV[0]
"#;

/// `example`'s rendered outputs written under `dir`, as a copy of the project has them, with the
/// example's own folder as the project's root (`ISOLOOM_TEST_ROOT`), and Vagrant's
/// `vagrant/util/is_port_open` (which the Vagrantfile requires on a Mac) empty for `ruby -I dir`:
/// the Vagrantfile's path, and `ruby` ready to load it.
pub(crate) fn loadable(example: &str, dir: &Path) -> Outcome<(std::path::PathBuf, Command)> {
    let mut vagrantfile = None;
    for f in render(example)? {
        let path = dir.join(&f.path);
        std::fs::create_dir_all(path.parent().ok_or("a file at the root")?)?;
        // The project's root: the example's own folder (the outputs are written elsewhere).
        let contents = f
            .contents
            .replace("\nROOT = File.expand_path(\"../..\", __dir__)\n", "\nROOT = ENV.fetch(\"ISOLOOM_TEST_ROOT\")\n");
        std::fs::write(&path, contents)?;
        if f.path.ends_with("/Vagrantfile") {
            vagrantfile = Some(path);
        }
    }
    std::fs::create_dir_all(dir.join("vagrant/util"))?;
    std::fs::write(dir.join("vagrant/util/is_port_open.rb"), "")?;
    let mut ruby = Command::new("ruby");
    ruby.arg("-I").arg(dir);
    ruby.env(
        "ISOLOOM_TEST_ROOT",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    );
    Ok((vagrantfile.ok_or("no Vagrantfile")?, ruby))
}

/// What `ruby` printed, or why it failed.
pub(crate) fn printed(mut ruby: Command) -> Outcome<String> {
    let out = ruby.output()?;
    if !out.status.success() {
        return Err(format!("ruby failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    // Ruby's standard output is in text mode on Windows: each line ends in "\r\n" there.
    Ok(String::from_utf8(out.stdout)?.replace("\r\n", "\n"))
}

/// A Ruby call's keyword arguments (`key: value`), each value's text as written: a string's
/// contents (its `#{…}` left as it is), any other value as its literal.
/// Keyword `key` of `kw`, empty when it has none.
pub(crate) fn field<'k>(kw: &'k std::collections::BTreeMap<String, String>, key: &str) -> &'k str {
    kw.get(key).map_or("", String::as_str)
}

pub(crate) fn keywords(call: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut rest = call;
    while let Some(after_comma) = rest.find(", ").and_then(|i| rest.get(i + 2..)) {
        let Some((key, after)) = after_comma.split_once(": ") else { break };
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            rest = after_comma;
            continue;
        }
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            string_literal(quoted)
        } else {
            let end = after.find(", ").unwrap_or(after.len());
            (after.get(..end).unwrap_or(after).trim().to_owned(), after.get(end..).unwrap_or(""))
        };
        out.insert(key.to_owned(), value);
        rest = tail;
    }
    out
}

/// The string literal starting at `s` (after its opening quote): its contents, and what follows
/// its closing quote (the first `"` not escaped and not inside a `#{…}`).
fn string_literal(s: &str) -> (String, &str) {
    let (mut value, mut depth, mut escaped) = (String::new(), 0_u32, false);
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '#' if chars.peek().is_some_and(|(_, n)| *n == '{') => depth += 1,
            '}' if depth > 0 => depth -= 1,
            '"' if depth == 0 => return (value, s.get(i + 1..).unwrap_or("")),
            _ => {}
        }
        value.push(c);
    }
    (value, "")
}

/// Each VM's private networks as libvirt sees them: (VM, the network's keyword arguments).
pub(crate) fn libvirt_networks(vagrantfile: &str) -> Vec<(String, std::collections::BTreeMap<String, String>)> {
    let file = isoloom_test_support::ruby_blocks(vagrantfile);
    let mut out = Vec::new();
    for vm in file.find("config.vm.define ") {
        let name = vm.head.split('"').nth(1).unwrap_or_default().to_owned();
        for call in vm.statements("m.vm.network \"private_network\"") {
            let kw = keywords(call);
            if kw.contains_key("libvirt__network_name") {
                out.push((name.clone(), kw));
            }
        }
    }
    out
}
