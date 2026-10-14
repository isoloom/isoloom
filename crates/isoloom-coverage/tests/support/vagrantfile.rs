//! Reads a generated Vagrantfile back into the coverage rows' keys: every setting, option,
//! network type, provisioner and provider setting it writes.

use std::collections::BTreeSet;

/// What a Vagrantfile writes, as coverage keys (`config.vm.network private_network: ip`).
/// `common` says whether an option is one every provisioner takes (`config.vm.provision *: run`).
pub fn written(text: &str, common: &dyn Fn(&str) -> bool) -> BTreeSet<String> {
    let mut scan = Scan {
        out: BTreeSet::new(),
        provider: None,
        common,
    };
    let body = text.split_once("Vagrant.configure(").map_or(text, |(_, b)| b);
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(tag) = heredoc(line) {
            // A heredoc's text is a script, not Ruby: skip to its closing tag.
            lines.by_ref().find(|l| l.trim() == tag);
        }
        scan.line(line);
    }
    scan.out
}

struct Scan<'a> {
    out: BTreeSet<String>,
    /// Inside `config.vm.provider "x" do |v|`: its name, and the depth of blocks opened since.
    provider: Option<(String, usize)>,
    common: &'a dyn Fn(&str) -> bool,
}

impl Scan<'_> {
    fn line(&mut self, line: &str) {
        if let Some((name, depth)) = &mut self.provider {
            if line == "end" {
                if *depth == 0 {
                    self.provider = None;
                } else {
                    *depth -= 1;
                }
            } else if opens_block(line) {
                *depth += 1;
            } else if let Some(rest) = line.strip_prefix("v.") {
                self.out.insert(format!("provider {name}: {}", ident(rest)));
            }
        }
        let Some(rest) = ["config.", "m.", "o."].iter().find_map(|p| line.strip_prefix(p)) else {
            return;
        };
        let (block, rest) = rest.split_at(ident(rest).len());
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        match block {
            "vm" => self.vm(rest),
            "trigger" => {
                self.out.insert("config.trigger".to_string());
            }
            _ => {
                self.out.insert(format!("config.{block}"));
                self.out.insert(format!("config.{block}.{}", ident(rest)));
            }
        }
    }

    /// `config.vm.<name> ...`: a machine setting, with its type and options where it has them.
    fn vm(&mut self, rest: &str) {
        let name = ident(rest);
        let args = &rest[name.len()..];
        let kind = args.split('"').nth(1).unwrap_or_default();
        self.out.insert(format!("config.vm.{name}"));
        match name {
            "network" | "provision" => {
                self.out.insert(format!("config.vm.{name} {kind}"));
                for key in keys(args) {
                    let scope = if name == "provision" && (self.common)(&key) { "*" } else { kind };
                    self.out.insert(format!("config.vm.{name} {scope}: {key}"));
                }
            }
            "disk" | "synced_folder" => {
                if args.trim_start().starts_with(':') {
                    self.out.insert(format!("config.vm.{name}: type"));
                }
                for key in keys(args) {
                    self.out.insert(format!("config.vm.{name}: {key}"));
                }
            }
            "provider" => self.provider = Some((kind.to_string(), 0)),
            _ => {}
        }
    }
}

/// The leading identifier of `s` (`memory` in `memory = 512`).
fn ident(s: &str) -> &str {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len());
    &s[..end]
}

/// Whether a line inside a provider block opens one of its own (closed by a later `end`).
fn opens_block(line: &str) -> bool {
    let code = code(line);
    let code = code.trim_end();
    ["if ", "unless ", "case ", "while "].iter().any(|k| code.starts_with(k)) || code.ends_with(" do") || code.contains(" do |")
}

/// The closing tag of a heredoc the line opens (`SH` for `<<~'SH'`).
fn heredoc(line: &str) -> Option<String> {
    let rest = &line[line.find("<<~")? + 3..];
    let tag: String = rest
        .trim_start_matches(['\'', '"'])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!tag.is_empty()).then_some(tag)
}

/// A line's code: string literals emptied, a trailing comment cut.
fn code(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => {
                quote = None;
                out.push(c);
            }
            (Some(_), _) => {}
            (None, '"' | '\'') => {
                quote = Some(c);
                out.push(c);
            }
            (None, '#') => break,
            (None, c) => out.push(c),
        }
    }
    out
}

/// The keyword arguments of a call (`ip` and `netmask` in `, ip: "x", netmask: "y"`).
fn keys(args: &str) -> Vec<String> {
    let code = code(args);
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    for (i, _) in code.match_indices(':') {
        let is_key = bytes.get(i + 1) != Some(&b':') && i > 0 && bytes[i - 1] != b':';
        let start = code[..i].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map_or(0, |p| p + 1);
        if is_key && start < i && bytes.get(start.wrapping_sub(1)) != Some(&b':') {
            out.push(code[start..i].to_string());
        }
    }
    out
}
