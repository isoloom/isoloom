//! The one Ruby builder every Vagrantfile is written with: code is assembled from typed pieces,
//! every string goes through [`rb`] (so no value can end a literal or interpolate), and the
//! indentation follows the block nesting instead of being spelled out in format strings.
//!
//! - [`Value`]: one Ruby expression. [`Value::str`] quotes text, [`Value::raw`] takes code
//!   as written (`ENV.fetch("X", "")`, `INPUTS`), plus integers, booleans, symbols, arrays and
//!   hashes built from other values.
//! - [`Call`]: a method call without parentheses, as Vagrantfiles write them:
//!   `config.vm.network "forwarded_port", guest: 80, host: 8080`. Positional arguments
//!   ([`Call::arg`]) come before keyword ones ([`Call::kw`]); a trailing `if` modifier
//!   ([`Call::when`]) or comment ([`Call::comment`]) is allowed.
//! - [`Ruby`]: the writer. [`Ruby::line`], [`Ruby::assign`] and [`Ruby::call`] write one
//!   statement at the current depth; [`Ruby::block`] writes `head do |params|`, the body one level
//!   deeper, and `end`; [`Ruby::heredoc`] passes a script as a squiggly heredoc (`<<~'SH'`, nothing
//!   expanded); [`Ruby::hash`] writes a constant hash one entry a line; [`Ruby::text`] adds
//!   prepared Ruby (a library such as the project copier) as it is.
//!
//! ```
//! use isoloom_vagrant::ruby::{Call, Ruby, Value};
//!
//! let mut r = Ruby::new();
//! r.block("Vagrant.configure(\"2\")", "config", |r| {
//!     r.assign("config.vm.box", Value::str("bento/debian-12"));
//!     r.block(Call::new("config.vm.provider").arg(Value::str("virtualbox")), "v", |r| {
//!         r.assign("v.cpus", 2u32);
//!     });
//!     r.call(Call::new("config.vm.provision").arg(Value::str("shell")).kw("inline", Value::str("echo hi")));
//! });
//! assert_eq!(
//!     r.finish(),
//!     "Vagrant.configure(\"2\") do |config|\n  config.vm.box = \"bento/debian-12\"\n  config.vm.provider \"virtualbox\" do |v|\n    v.cpus = 2\n  end\n  config.vm.provision \"shell\", inline: \"echo hi\"\nend\n"
//! );
//! ```

use std::fmt;

pub use isoloom_render::rb;

/// One Ruby expression, already written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value(String);

impl Value {
    /// A string literal holding `text`, quoted by [`rb`].
    pub fn str(text: &str) -> Value {
        Value(rb(text))
    }

    /// Ruby code as written: an expression Isoloom composes itself, never user text.
    pub fn raw(code: impl Into<String>) -> Value {
        Value(code.into())
    }

    /// A symbol (`:windows`).
    pub fn sym(name: &str) -> Value {
        Value(format!(":{name}"))
    }

    /// An array literal (`[a, b]`).
    pub fn array(items: impl IntoIterator<Item = Value>) -> Value {
        Value(format!("[{}]", join(items)))
    }

    /// A hash literal on one line (`{ k => v }`).
    pub fn hash(entries: impl IntoIterator<Item = (Value, Value)>) -> Value {
        let entries: Vec<String> = entries.into_iter().map(|(k, v)| format!("{k} => {v}")).collect();
        if entries.is_empty() {
            return Value("{}".into());
        }
        Value(format!("{{ {} }}", entries.join(", ")))
    }

    /// The expression's code.
    pub fn as_str(&self) -> &str {
        &self.0
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
pub struct Call {
    method: String,
    args: Vec<String>,
    condition: Option<String>,
    comment: Option<String>,
}

impl Call {
    /// A call of `method` (`config.vm.network`), with no arguments yet.
    pub fn new(method: impl Into<String>) -> Call {
        Call {
            method: method.into(),
            args: Vec::new(),
            condition: None,
            comment: None,
        }
    }

    /// Adds a positional argument (they come first: add them before keyword ones).
    #[must_use]
    pub fn arg(mut self, value: impl Into<Value>) -> Call {
        self.args.push(value.into().0);
        self
    }

    /// Adds a keyword argument (`key: value`).
    #[must_use]
    pub fn kw(mut self, key: &str, value: impl Into<Value>) -> Call {
        self.args.push(format!("{key}: {}", value.into()));
        self
    }

    /// Adds a keyword argument when there is a value.
    #[must_use]
    pub fn kw_opt(self, key: &str, value: Option<Value>) -> Call {
        match value {
            Some(v) => self.kw(key, v),
            None => self,
        }
    }

    /// Runs the call only when `condition` (Ruby code) holds: `call if condition`.
    #[must_use]
    pub fn when(mut self, condition: impl Into<String>) -> Call {
        self.condition = Some(condition.into());
        self
    }

    /// Ends the line with a comment.
    #[must_use]
    pub fn comment(mut self, text: impl Into<String>) -> Call {
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
        if let Some(c) = &self.condition {
            write!(f, " if {c}")?;
        }
        if let Some(c) = &self.comment {
            write!(f, " # {c}")?;
        }
        Ok(())
    }
}

/// Ruby source, written statement by statement at the depth of the enclosing blocks (two
/// spaces a level).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ruby {
    out: String,
    depth: usize,
}

impl Ruby {
    /// An empty file.
    pub fn new() -> Ruby {
        Ruby::default()
    }

    /// Source that will sit `depth` blocks deep in another file (a VM block written apart).
    pub fn at_depth(depth: usize) -> Ruby {
        Ruby { out: String::new(), depth }
    }

    /// Prepared Ruby, added as it is (its own indentation and line ends).
    pub fn text(&mut self, ruby: &str) -> &mut Ruby {
        self.out.push_str(ruby);
        self
    }

    /// One line of code at the current depth.
    pub fn line(&mut self, code: impl fmt::Display) -> &mut Ruby {
        self.indent();
        self.out.push_str(&code.to_string());
        self.out.push('\n');
        self
    }

    /// An empty line.
    pub fn blank(&mut self) -> &mut Ruby {
        self.out.push('\n');
        self
    }

    /// A comment, one `#` line per line of `text`.
    pub fn comment(&mut self, text: &str) -> &mut Ruby {
        for l in text.lines() {
            self.line(format_args!("# {l}"));
        }
        self
    }

    /// `target = value`.
    pub fn assign(&mut self, target: &str, value: impl Into<Value>) -> &mut Ruby {
        self.line(format_args!("{target} = {}", value.into()))
    }

    /// `target = value if condition`.
    pub fn assign_if(&mut self, target: &str, value: impl Into<Value>, condition: &str) -> &mut Ruby {
        self.line(format_args!("{target} = {} if {condition}", value.into()))
    }

    /// A statement that is a call.
    pub fn call(&mut self, call: Call) -> &mut Ruby {
        self.line(call)
    }

    /// `head do |params|`, `body` one level deeper, then `end` (no `|...|` when `params` is
    /// empty).
    pub fn block(&mut self, head: impl fmt::Display, params: &str, body: impl FnOnce(&mut Ruby)) -> &mut Ruby {
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

    /// `call` with a last keyword argument `key: <<~'TAG'`, then `script` one level deeper and the
    /// closing tag. The quoted tag keeps Ruby from expanding anything in the script.
    pub fn heredoc(&mut self, call: Call, key: &str, tag: &str, script: &str) -> &mut Ruby {
        self.call(call.kw(key, Value::raw(format!("<<~'{tag}'"))));
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

    /// `target = {`, one `key => value,` a line, `}`.
    pub fn hash(&mut self, target: &str, entries: impl IntoIterator<Item = (Value, Value)>) -> &mut Ruby {
        self.line(format_args!("{target} = {{"));
        self.depth += 1;
        for (k, v) in entries {
            self.line(format_args!("{k} => {v},"));
        }
        self.depth -= 1;
        self.line("}")
    }

    /// The source written so far.
    pub fn finish(self) -> String {
        self.out
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Call, Ruby, Value};

    #[test]
    fn strings_are_quoted_and_cannot_interpolate() {
        assert_eq!(Value::str("a \"b\" #{c}").as_str(), r#""a \"b\" \#{c}""#);
        assert_eq!(Value::raw("ENV[\"X\"]").as_str(), "ENV[\"X\"]");
    }

    #[test]
    fn values_compose() {
        assert_eq!(Value::sym("windows").as_str(), ":windows");
        assert_eq!(Value::array([Value::str("a"), 2u16.into()]).as_str(), "[\"a\", 2]");
        assert_eq!(Value::hash([(Value::str("k"), true.into())]).as_str(), "{ \"k\" => true }");
        assert_eq!(Value::hash([]).as_str(), "{}");
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
        assert_eq!(Call::new("x").when("y").to_string(), "x if y");
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
        let mut r = Ruby::at_depth(2);
        r.heredoc(Call::new("m.vm.provision").arg(Value::str("shell")), "inline", "SH", "set -e\n\necho hi\n");
        assert_eq!(
            r.finish(),
            "    m.vm.provision \"shell\", inline: <<~'SH'\n      set -e\n\n      echo hi\n    SH\n"
        );
    }

    #[test]
    fn a_hash_constant_lists_one_entry_a_line() {
        let mut r = Ruby::new();
        r.comment("Inputs.").hash("INPUTS", [(Value::str("A"), Value::raw("ENV.fetch(\"A\", \"\")"))]);
        assert_eq!(r.finish(), "# Inputs.\nINPUTS = {\n  \"A\" => ENV.fetch(\"A\", \"\"),\n}\n");
    }

    #[test]
    fn text_goes_in_as_it_is() {
        let mut r = Ruby::at_depth(3);
        r.text("x\n").blank();
        assert_eq!(r.finish(), "x\n\n");
    }
}
