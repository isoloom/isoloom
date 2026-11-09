//! The one Ruby builder every Vagrantfile is written with: code is assembled from typed pieces,
//! every string goes through [`ruby::quote`] (so no value can end a literal or interpolate), and the
//! indentation follows the block nesting instead of being spelled out in format strings.
//!
//! - [`Value`]: one Ruby expression. [`Value::str`] quotes text, [`Value::raw`] takes code
//!   as written (`ENV.fetch("X", "")`, `INPUTS`), plus integers, booleans, symbols, arrays and
//!   hashes built from other values.
//! - [`Call`]: a method call without parentheses, as Vagrantfiles write them:
//!   `config.vm.network "forwarded_port", guest: 80, host: 8080`. Positional arguments
//!   ([`Call::arg`]) come before keyword ones ([`Call::kw`]); a trailing comment
//!   ([`Call::comment`]) is allowed.
//! - [`Ruby`]: the writer. [`Ruby::line`], [`Ruby::assign`] and [`Ruby::call`] write one
//!   statement at the current depth; [`Ruby::block`] writes `head do |params|`, the body one level
//!   deeper, and `end`; [`Ruby::hash`] writes a constant hash one entry a line; [`Ruby::text`]
//!   adds prepared Ruby (a library such as the project copier) as it is.
//! - [`Provision`]: a provisioner, its script a string or a squiggly heredoc (`<<~'SH'`, nothing
//!   expanded, its tag never a line of the script); [`Ruby::provision`] writes it.
//! - [`Ruby::configure`] writes the `Vagrant.configure` block and [`Ruby::define`] a VM's
//!   `config.vm.define` block in it; as they are written, the builder records what Vagrant
//!   applies to each VM only when it boots ([`BootStatements`]): every statement of the configure
//!   block but comments and provisioners (known by their type, never by their text), those of a
//!   define for its VM, the others for every VM.

use std::fmt;

use isoloom_render::ruby;

/// One Ruby expression, already written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Value(String);

impl Value {
    /// A string literal holding `text`, quoted by [`ruby::quote`].
    pub(crate) fn str(text: &str) -> Value {
        Value(ruby::quote(text))
    }

    /// Ruby code as written: an expression Isoloom composes itself, never user text.
    pub(crate) fn raw(code: impl Into<String>) -> Value {
        Value(code.into())
    }

    /// A symbol (`:windows`).
    pub(crate) fn sym(name: &str) -> Value {
        Value(format!(":{name}"))
    }

    /// An array literal (`[a, b]`).
    pub(crate) fn array(items: impl IntoIterator<Item = Value>) -> Value {
        Value(format!("[{}]", join(items)))
    }

    /// A hash literal on one line (`{ k => v }`).
    pub(crate) fn hash(entries: impl IntoIterator<Item = (Value, Value)>) -> Value {
        let entries: Vec<String> = entries.into_iter().map(|(k, v)| format!("{k} => {v}")).collect();
        if entries.is_empty() {
            return Value("{}".into());
        }
        Value(format!("{{ {} }}", entries.join(", ")))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<u16> for Value {
    fn from(n: u16) -> Value {
        Value(n.to_string())
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Value {
        Value(n.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value(b.to_string())
    }
}

fn join(items: impl IntoIterator<Item = Value>) -> String {
    items.into_iter().map(|v| v.0).collect::<Vec<_>>().join(", ")
}

/// A method call written without parentheses: `receiver.method arg, key: value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    method: String,
    args: Vec<String>,
    comment: Option<String>,
}

impl Call {
    /// A call of `method` (`config.vm.network`), with no arguments yet.
    pub(crate) fn new(method: impl Into<String>) -> Call {
        Call {
            method: method.into(),
            args: Vec::new(),
            comment: None,
        }
    }

    /// Adds a positional argument (they come first: add them before keyword ones).
    #[must_use]
    pub(crate) fn arg(mut self, value: impl Into<Value>) -> Call {
        self.args.push(value.into().0);
        self
    }

    /// Adds a keyword argument (`key: value`).
    #[must_use]
    pub(crate) fn kw(mut self, key: &str, value: impl Into<Value>) -> Call {
        self.args.push(format!("{key}: {}", value.into()));
        self
    }

    /// Adds a keyword argument when there is a value.
    #[must_use]
    pub(crate) fn kw_opt(self, key: &str, value: Option<Value>) -> Call {
        match value {
            Some(v) => self.kw(key, v),
            None => self,
        }
    }

    /// Ends the line with a comment.
    #[must_use]
    pub(crate) fn comment(mut self, text: impl Into<String>) -> Call {
        self.comment = Some(text.into());
        self
    }
}

impl fmt::Display for Call {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.method)?;
        if !self.args.is_empty() {
            write!(f, " {}", self.args.join(", "))?;
        }
        if let Some(c) = &self.comment {
            write!(f, " # {c}")?;
        }
        Ok(())
    }
}

/// A provisioner: `<var>.vm.provision "<name>", key: value`, with its script as a keyword
/// argument (a string, or a heredoc after the call). [`Ruby::provision`] writes it; being one by
/// construction, it is never recorded as what a VM boots with (`vagrant up --provision` runs it on
/// a running VM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Provision {
    call: Call,
    /// The heredoc the call ends with: its keyword, its tag, the script.
    heredoc: Option<(String, String, String)>,
}

impl Provision {
    /// `<var>.vm.provision "<name>"` (`"shell"`, or a name `--provision-with` takes, with its
    /// `type:`), no other argument yet.
    pub(crate) fn new(var: &str, name: &str) -> Provision {
        Provision {
            call: Call::new(format!("{var}.vm.provision")).arg(Value::str(name)),
            heredoc: None,
        }
    }

    /// Adds a keyword argument (`key: value`).
    #[must_use]
    pub(crate) fn kw(mut self, key: &str, value: impl Into<Value>) -> Provision {
        self.call = self.call.kw(key, value);
        self
    }

    /// Adds a keyword argument when there is a value.
    #[must_use]
    pub(crate) fn kw_opt(mut self, key: &str, value: Option<Value>) -> Provision {
        self.call = self.call.kw_opt(key, value);
        self
    }

    /// `script` as the last keyword argument `key:`, as a squiggly heredoc tagged `tag` (`<<~'SH'`,
    /// nothing expanded; its closing tag never a line of the script, see [`closing_tag`]).
    #[must_use]
    pub(crate) fn heredoc(mut self, key: &str, tag: &str, script: &str) -> Provision {
        self.heredoc = Some((key.to_owned(), closing_tag(tag, script), script.to_owned()));
        self
    }

    /// `script` as the last keyword argument `key:`: a quoted string when it is one line, else
    /// a [heredoc](Provision::heredoc) tagged `SH`.
    #[must_use]
    pub(crate) fn script(self, key: &str, script: &str) -> Provision {
        let body = script.trim_end_matches('\n');
        if body.contains('\n') {
            self.heredoc(key, "SH", body)
        } else {
            self.kw(key, Value::str(body))
        }
    }
}

/// Ruby source, written statement by statement at the depth of the enclosing blocks (two
/// spaces a level).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ruby {
    out: String,
    depth: usize,
    /// What the configure block being written applies at boot, while it is written.
    recording: Option<Recording>,
    /// What the configure block applied at boot, once written.
    boot: BootStatements,
}

/// The statements of a Vagrantfile's configure block that Vagrant applies to a VM only when it
/// boots it (`vagrant up` of a stopped VM, `vagrant reload`): synced folders, networks, forwarded
/// ports, size, provider settings. Comments and provisioners ([`Provision`], which `vagrant up
/// --provision` runs on a running VM) are left out; each statement is indented to its depth in
/// its scope. Nothing outside the configure block is recorded: what a VM boots with must be
/// written in it, never read from a constant the spec sets above it (the ESXi port groups are
/// each define's own indexes, not a lookup in a list of the networks). The constants above it
/// that boot settings read come from the host running Vagrant, not from the spec: `ROOT`,
/// `HOST_ARCH`, `ESXI_NETWORKS`, `HYPERV_SWITCH` and the host ports the runner chose
/// (`ISOLOOM_HOSTPORTS`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BootStatements {
    /// The configure block's own statements, outside every define: what all its VMs share.
    pub(crate) shared: Vec<String>,
    /// Each define's statements, by its VM's name, in the file's order.
    pub(crate) defines: Vec<(String, Vec<String>)>,
}

/// The configure block being written: the statements so far and the scope a statement lands in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Recording {
    statements: BootStatements,
    /// The depth of the configure block's body.
    base: usize,
    /// The define being written, with the depth of its body.
    define: Option<(usize, usize)>,
}

impl Ruby {
    /// An empty file.
    pub(crate) fn new() -> Ruby {
        Ruby::default()
    }

    /// Prepared Ruby, added as it is (its own indentation and line ends): a library outside the
    /// configure block, so nothing Vagrant applies at boot is recorded from it.
    pub(crate) fn text(&mut self, ruby: &str) -> &mut Ruby {
        self.out.push_str(ruby);
        self
    }

    /// One line of code at the current depth, recorded when it is a statement Vagrant applies at
    /// boot ([`BootStatements`]).
    pub(crate) fn line(&mut self, code: impl fmt::Display) -> &mut Ruby {
        let code = code.to_string();
        self.record(&code);
        self.write(&code)
    }

    /// One line at the current depth, not recorded.
    fn write(&mut self, code: &str) -> &mut Ruby {
        self.indent();
        self.out.push_str(code);
        self.out.push('\n');
        self
    }

    /// `code`, a statement at the current depth, in the scope it is written in (a define, else
    /// the configure block's own), unless no configure block is being written.
    fn record(&mut self, code: &str) {
        let depth = self.depth;
        let Some(rec) = self.recording.as_mut() else {
            return;
        };
        let (scope, at) = match rec.define {
            Some((i, at)) => (rec.statements.defines.get_mut(i).map(|(_, s)| s), at),
            None => (Some(&mut rec.statements.shared), rec.base),
        };
        if let Some(scope) = scope {
            scope.push(format!("{}{code}", "  ".repeat(depth.saturating_sub(at))));
        }
    }

    /// `Vagrant.configure("2") do |config|`, `body` one level deeper, then `end`, recording what
    /// Vagrant applies at boot ([`Ruby::boot`]).
    pub(crate) fn configure(&mut self, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        self.write("Vagrant.configure(\"2\") do |config|");
        self.depth += 1;
        self.recording = Some(Recording {
            base: self.depth,
            ..Recording::default()
        });
        body(self);
        if let Some(rec) = self.recording.take() {
            self.boot = rec.statements;
        }
        self.depth -= 1;
        self.write("end")
    }

    /// `config.vm.define "<name>" do |m|` (`autostart: false` unless `autostart`), `body` one
    /// level deeper, then `end`: VM `name`, whose statements are recorded as its own.
    pub(crate) fn define(&mut self, name: &str, autostart: bool, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        let mut call = Call::new("config.vm.define").arg(Value::str(name));
        if !autostart {
            call = call.kw("autostart", false);
        }
        self.write(&format!("{call} do |m|"));
        self.depth += 1;
        let outer = self.recording.as_mut().map(|rec| {
            rec.statements.defines.push((name.to_owned(), Vec::new()));
            rec.define.replace((rec.statements.defines.len() - 1, self.depth))
        });
        body(self);
        if let (Some(rec), Some(outer)) = (self.recording.as_mut(), outer) {
            rec.define = outer;
        }
        self.depth -= 1;
        self.write("end")
    }

    /// What the configure block written applies at boot (empty before [`Ruby::configure`]).
    pub(crate) fn boot(&self) -> &BootStatements {
        &self.boot
    }

    /// An empty line.
    pub(crate) fn blank(&mut self) -> &mut Ruby {
        self.out.push('\n');
        self
    }

    /// A comment, one `#` line per line of `text` (never recorded).
    pub(crate) fn comment(&mut self, text: &str) -> &mut Ruby {
        for l in text.lines() {
            self.write(&format!("# {l}"));
        }
        self
    }

    /// `target = value`.
    pub(crate) fn assign(&mut self, target: &str, value: impl Into<Value>) -> &mut Ruby {
        self.line(format_args!("{target} = {}", value.into()))
    }

    /// `target = value if condition`.
    pub(crate) fn assign_if(&mut self, target: &str, value: impl Into<Value>, condition: &str) -> &mut Ruby {
        self.line(format_args!("{target} = {} if {condition}", value.into()))
    }

    /// A statement that is a call.
    pub(crate) fn call(&mut self, call: Call) -> &mut Ruby {
        self.line(call)
    }

    /// `head do |params|`, `body` one level deeper, then `end` (no `|...|` when `params` is
    /// empty).
    pub(crate) fn block(&mut self, head: impl fmt::Display, params: &str, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        let head = head.to_string();
        if params.is_empty() {
            self.line(format_args!("{head} do"));
        } else {
            self.line(format_args!("{head} do |{params}|"));
        }
        self.depth += 1;
        body(self);
        self.depth -= 1;
        self.line("end")
    }

    /// A provisioner: its call, then its heredoc's lines one level deeper and its closing tag,
    /// when it has one. Never recorded ([`BootStatements`]).
    pub(crate) fn provision(&mut self, p: Provision) -> &mut Ruby {
        let recording = self.recording.take();
        match p.heredoc {
            None => self.line(p.call),
            Some((key, tag, script)) => {
                self.line(p.call.kw(&key, Value::raw(format!("<<~'{tag}'"))));
                self.heredoc_body(&tag, &script)
            }
        };
        self.recording = recording;
        self
    }

    /// A line of code passing the heredoc as an argument, `{open}<<~'TAG'{close}`
    /// (`IsoloomHyperV.address(machine, <<~'SH')`), then `script` one level deeper and the closing
    /// tag ([`closing_tag`] of `tag`).
    pub(crate) fn heredoc_argument(&mut self, open: &str, close: &str, tag: &str, script: &str) -> &mut Ruby {
        let tag = closing_tag(tag, script);
        self.line(format_args!("{open}<<~'{tag}'{close}"));
        self.heredoc_body(&tag, script)
    }

    /// A heredoc's lines one level deeper, then its closing tag.
    fn heredoc_body(&mut self, tag: &str, script: &str) -> &mut Ruby {
        self.depth += 1;
        for l in script.lines() {
            if l.is_empty() {
                self.blank();
            } else {
                self.line(l);
            }
        }
        self.depth -= 1;
        self.line(tag)
    }

    /// `if condition`, `body` one level deeper, then `end`.
    pub(crate) fn if_block(&mut self, condition: impl fmt::Display, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        self.line(format_args!("if {condition}"));
        self.depth += 1;
        body(self);
        self.depth -= 1;
        self.line("end")
    }

    /// `target = {`, one `key => value,` a line, `}`.
    pub(crate) fn hash(&mut self, target: &str, entries: impl IntoIterator<Item = (Value, Value)>) -> &mut Ruby {
        self.line(format_args!("{target} = {{"));
        self.depth += 1;
        for (k, v) in entries {
            self.line(format_args!("{k} => {v},"));
        }
        self.depth -= 1;
        self.line("}")
    }

    /// The source written so far.
    pub(crate) fn finish(self) -> String {
        self.out
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }
}

/// A heredoc's closing tag: `tag`, else `ISOLOOM_<tag>`, `ISOLOOM_<tag>_2`... the first that no
/// line of `script` is. A squiggly heredoc ends at the first line holding only its tag, however
/// indented, so a script with that line would end it early and run the rest as Ruby.
fn closing_tag(tag: &str, script: &str) -> String {
    let taken = |candidate: &str| script.lines().any(|l| l.trim() == candidate);
    if !taken(tag) {
        return tag.to_owned();
    }
    // One more candidate than the script has lines: one of them is free.
    let isoloom = format!("ISOLOOM_{tag}");
    std::iter::once(isoloom.clone())
        .chain((2..=script.lines().count() + 1).map(|n| format!("{isoloom}_{n}")))
        .find(|candidate| !taken(candidate))
        .unwrap_or(isoloom)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Call, Provision, Ruby, Value, closing_tag};

    #[test]
    fn a_vagrantfile_is_written_from_typed_pieces() {
        let mut r = Ruby::new();
        r.block("Vagrant.configure(\"2\")", "config", |r| {
            r.assign("config.vm.box", Value::str("bento/debian-12"));
            r.block(Call::new("config.vm.provider").arg(Value::str("virtualbox")), "v", |r| {
                r.assign("v.cpus", 2u32);
            });
            r.provision(Provision::new("config", "shell").kw("inline", Value::str("echo hi")));
        });
        assert_eq!(
            r.finish(),
            "Vagrant.configure(\"2\") do |config|\n  config.vm.box = \"bento/debian-12\"\n  config.vm.provider \"virtualbox\" do |v|\n    v.cpus = 2\n  end\n  config.vm.provision \"shell\", inline: \"echo hi\"\nend\n"
        );
    }

    #[test]
    fn a_heredoc_tag_never_is_a_line_of_its_script() {
        assert_eq!(closing_tag("SH", "a\nb"), "SH");
        assert_eq!(closing_tag("SH", "a\n  SH\nb"), "ISOLOOM_SH");
        assert_eq!(closing_tag("SH", "SH\nISOLOOM_SH\nISOLOOM_SH_2"), "ISOLOOM_SH_3");
        let mut r = Ruby::new();
        r.heredoc_argument("f(", ")", "PS1", "PS1\nx");
        assert_eq!(r.finish(), "f(<<~'ISOLOOM_PS1')\n  PS1\n  x\nISOLOOM_PS1\n");
    }

    proptest! {
        #[test]
        fn a_heredoc_ends_only_at_its_own_closing_line(lines in prop::collection::vec("( *)(SH|ISOLOOM_SH|ISOLOOM_SH_2|[a-z]{1,6})", 2..8)) {
            let script = lines.join("\n");
            let mut r = Ruby::new();
            r.provision(Provision::new("p", "shell").script("inline", &script));
            let written = r.finish();
            let rows: Vec<&str> = written.lines().collect();
            let (first, rest) = rows.split_first().expect("the call");
            let tag = first.split("<<~'").nth(1).and_then(|t| t.strip_suffix('\'')).expect("a heredoc");
            let (last, body) = rest.split_last().expect("the closing line");
            prop_assert_eq!(*last, tag);
            prop_assert!(body.iter().all(|l| l.trim() != tag), "{written}");
            prop_assert_eq!(body.len(), lines.len());
        }
    }

    #[test]
    fn strings_are_quoted_and_cannot_interpolate() {
        assert_eq!(Value::str("a \"b\" #{c}").to_string(), r#""a \"b\" \#{c}""#);
        assert_eq!(Value::raw("ENV[\"X\"]").to_string(), "ENV[\"X\"]");
    }

    #[test]
    fn values_compose() {
        assert_eq!(Value::sym("windows").to_string(), ":windows");
        assert_eq!(Value::array([Value::str("a"), 2u16.into()]).to_string(), "[\"a\", 2]");
        assert_eq!(Value::hash([(Value::str("k"), true.into())]).to_string(), "{ \"k\" => true }");
        assert_eq!(Value::hash([]).to_string(), "{}");
    }

    #[test]
    fn a_call_lists_positional_then_keyword_arguments() {
        let c = Call::new("m.vm.network")
            .arg(Value::str("forwarded_port"))
            .kw("guest", 80u16)
            .kw_opt("protocol", None)
            .kw_opt("id", Some(Value::str("web")))
            .comment("why");
        assert_eq!(c.to_string(), "m.vm.network \"forwarded_port\", guest: 80, id: \"web\" # why");
    }

    #[test]
    fn blocks_nest_and_indent() {
        let mut r = Ruby::new();
        r.block("a", "", |r| {
            r.block(Call::new("b").arg(Value::str("c")), "v, o", |r| {
                r.assign_if("v.x", Value::raw("ENV[\"X\"]"), "ENV[\"X\"]");
            });
        });
        assert_eq!(r.finish(), "a do\n  b \"c\" do |v, o|\n    v.x = ENV[\"X\"] if ENV[\"X\"]\n  end\nend\n");
    }

    #[test]
    fn a_heredoc_indents_its_script_and_keeps_blank_lines_empty() {
        let mut r = Ruby::new();
        r.provision(Provision::new("m", "shell").heredoc("inline", "SH", "set -e\n\necho hi\n"));
        assert_eq!(r.finish(), "m.vm.provision \"shell\", inline: <<~'SH'\n  set -e\n\n  echo hi\nSH\n");
    }

    #[test]
    fn a_script_is_a_string_on_one_line_and_a_heredoc_on_more() {
        let mut r = Ruby::new();
        let p = || Provision::new("p", "shell");
        r.provision(p().script("inline", "echo \"hi\"\n"));
        r.provision(p().script("inline", "a\nb\n"));
        r.provision(p().script("inline", "cat <<SH\nx\nSH\n"));
        assert_eq!(
            r.finish(),
            "p.vm.provision \"shell\", inline: \"echo \\\"hi\\\"\"\np.vm.provision \"shell\", inline: <<~'SH'\n  a\n  b\nSH\np.vm.provision \"shell\", inline: <<~'ISOLOOM_SH'\n  cat <<SH\n  x\n  SH\nISOLOOM_SH\n"
        );
    }

    #[test]
    fn an_if_block_nests() {
        let mut r = Ruby::new();
        r.if_block("a != b", |r| {
            r.assign("x", 1u32);
        });
        assert_eq!(r.finish(), "if a != b\n  x = 1\nend\n");
    }

    #[test]
    fn a_hash_constant_lists_one_entry_a_line() {
        let mut r = Ruby::new();
        r.comment("Inputs.").hash("INPUTS", [(Value::str("A"), Value::raw("ENV.fetch(\"A\", \"\")"))]);
        assert_eq!(r.finish(), "# Inputs.\nINPUTS = {\n  \"A\" => ENV.fetch(\"A\", \"\"),\n}\n");
    }

    #[test]
    fn text_goes_in_as_it_is() {
        let mut r = Ruby::new();
        r.text("x\n").blank();
        assert_eq!(r.finish(), "x\n\n");
    }

    #[test]
    fn the_configure_block_records_what_each_vm_boots_with() {
        let mut r = Ruby::new();
        r.assign("ROOT", Value::raw("1"));
        r.configure(|r| {
            r.assign("config.vm.boot_timeout", 900u32);
            r.comment("shared");
            r.provision(Provision::new("config", "shell").heredoc("inline", "SH", "echo a\n"));
            r.define("web", false, |r| {
                r.block(Call::new("m.vm.provider").arg(Value::str("utm")), "_, o", |r| {
                    r.assign("o.memory", 512u32);
                });
                r.provision(Provision::new("m", "shell").script("inline", "b"));
            });
        });
        let boot = r.boot().clone();
        assert_eq!(boot.shared, ["config.vm.boot_timeout = 900"]);
        assert_eq!(
            boot.defines,
            [(
                "web".to_owned(),
                vec!["m.vm.provider \"utm\" do |_, o|".to_owned(), "  o.memory = 512".to_owned(), "end".to_owned()]
            )]
        );
        assert!(r.finish().contains("config.vm.define \"web\", autostart: false do |m|\n"));
    }
}
