//! The one Ruby builder every Vagrantfile is written with: code is assembled from typed pieces,
//! every string goes through [`quote`] (so no value can end a literal or interpolate), and the
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
//!   `config.vm.define` block in it.

use std::fmt;

/// A Ruby double-quoted string literal reading as `s`: backslashes and quotes escaped, and every
/// `#` that could start an interpolation (`#{...}`, `#$global`, `#@ivar`) made literal.
#[must_use]
pub fn quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace("#{", "\\#{")
        .replace("#$", "\\#$")
        .replace("#@", "\\#@");
    format!("\"{escaped}\"")
}

/// One Ruby expression, already written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Value(String);

impl Value {
    /// A string literal holding `text`, quoted by [`quote`].
    pub(crate) fn str(text: &str) -> Value {
        Value(quote(text))
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
/// argument (a string, or a heredoc after the call). [`Ruby::provision`] writes it.
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

    /// `<var>.vm.provision "<name>", type: "<kind>"`: a provisioner named at its own level, which
    /// `vagrant up --provision-with <name>` takes (it refuses a name given only as the shell
    /// provisioner's own `name:` option, and the project provisioner has no such option).
    pub(crate) fn named(var: &str, name: &str, kind: &str) -> Provision {
        Provision::new(var, name).kw("type", Value::str(kind))
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

/// When a VM [`Ruby::define`] writes is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    /// By `vagrant up`, with the environment.
    WithUp,
    /// Not by `vagrant up` (`autostart: false`): the runner brings it up on its own.
    OnDemand,
}

/// Ruby source, written statement by statement at the depth of the enclosing blocks (two
/// spaces a level).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ruby {
    out: String,
    depth: usize,
}

impl Ruby {
    /// An empty file.
    pub(crate) fn new() -> Ruby {
        Ruby::default()
    }

    /// Prepared Ruby, added as it is (its own indentation and line ends): a library outside the
    /// configure block.
    pub(crate) fn text(&mut self, ruby: &str) -> &mut Ruby {
        self.out.push_str(ruby);
        self
    }

    /// One line of code at the current depth.
    pub(crate) fn line(&mut self, code: impl fmt::Display) -> &mut Ruby {
        self.write(&code.to_string())
    }

    /// One line at the current depth.
    fn write(&mut self, code: &str) -> &mut Ruby {
        self.indent();
        self.out.push_str(code);
        self.out.push('\n');
        self
    }

    /// `Vagrant.configure("2") do |config|`, `body` one level deeper, then `end`.
    pub(crate) fn configure(&mut self, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        self.write("Vagrant.configure(\"2\") do |config|");
        self.depth += 1;
        body(self);
        self.depth -= 1;
        self.write("end")
    }

    /// `config.vm.define "<name>" do |m|` (`autostart: false` for a VM started [`Start::OnDemand`]),
    /// `body` one level deeper, then `end`: VM `name`.
    pub(crate) fn define(&mut self, name: &str, start: Start, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
        let mut call = Call::new("config.vm.define").arg(Value::str(name));
        if start == Start::OnDemand {
            call = call.kw("autostart", false);
        }
        self.write(&format!("{call} do |m|"));
        self.depth += 1;
        body(self);
        self.depth -= 1;
        self.write("end")
    }

    /// An empty line.
    pub(crate) fn blank(&mut self) -> &mut Ruby {
        self.out.push('\n');
        self
    }

    /// A comment, one `#` line per line of `text`.
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
    /// when it has one.
    pub(crate) fn provision(&mut self, p: Provision) -> &mut Ruby {
        match p.heredoc {
            None => self.line(p.call),
            Some((key, tag, script)) => {
                self.line(p.call.kw(&key, Value::raw(format!("<<~'{tag}'"))));
                self.heredoc_body(&tag, &script)
            }
        };
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
mod tests;
