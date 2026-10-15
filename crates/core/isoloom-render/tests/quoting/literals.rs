//! The literals the builders write, and decoders that read each back as its language would: a
//! decoder gives `None` for a literal the language would not read back as plain text (it ends
//! early, or something in it would expand). The quoting properties (`tests/quoting.rs`) and the fuzz
//! target (`fuzz/fuzz_targets/quoting.rs`) both check that every literal reads back as its text.

use isoloom_render::hcl_builder::{Block, Module, string};

/// The literal the HCL builder writes for a string value (`v = <literal>` in a block); `None`
/// when the module can't be written.
pub(crate) fn hcl_string(text: &str) -> Option<String> {
    let mut m = Module::new("");
    m.block(Block::builder("b").add_attribute(("v", string(text))).build());
    let written = m.render().ok()?;
    written.lines().find_map(|l| l.trim_start().strip_prefix("v = ")).map(str::to_owned)
}

/// A POSIX sh word made of single-quoted runs and backslash-escaped characters.
pub(crate) fn sh_decode(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => loop {
                match chars.next()? {
                    '\'' => break,
                    quoted => out.push(quoted),
                }
            },
            '\\' => out.push(chars.next()?),
            // Anything else outside quotes would be read by the shell: not a safe word.
            _ => return None,
        }
    }
    Some(out)
}

/// One piece of an HCL quoted template: text, or an interpolated expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// Text, as it reads.
    Text(String),
    /// The expression inside `${...}`, as written.
    Expr(String),
}

/// An HCL quoted template as its pieces, read as HCL's template scanner does, left to right:
/// `$${` and `%%{` are the literal markers, `${...}` an interpolation, a backslash an escape
/// (`\n`, `\r`, `\t`, `\\`, `\"`, `\uXXXX`). `None` for a bare `"`, a line break, a directive
/// (`%{`) or anything else HCL wouldn't read as one well-formed template.
pub(crate) fn hcl_pieces(lit: &str) -> Option<Vec<Piece>> {
    let mut rest = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("$${").or_else(|| rest.strip_prefix("%%{")) {
            text.push(c);
            text.push('{');
            rest = after;
        } else if let Some(after) = rest.strip_prefix("${") {
            let (expr, tail) = after.split_once('}')?;
            out.extend((!text.is_empty()).then(|| Piece::Text(std::mem::take(&mut text))));
            out.push(Piece::Expr(expr.to_owned()));
            rest = tail;
        } else if rest.starts_with("%{") || c == '"' || c == '\n' {
            return None;
        } else {
            let mut chars = rest.chars();
            chars.next();
            if c == '\\' {
                text.push(escape(&mut chars)?);
            } else {
                text.push(c);
            }
            rest = chars.as_str();
        }
    }
    out.extend((!text.is_empty()).then_some(Piece::Text(text)));
    Some(out)
}

/// The character an HCL escape stands for, `chars` just after its backslash.
fn escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
    Some(match chars.next()? {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '"' => '"',
        'u' => {
            let hex: String = chars.by_ref().take(4).collect();
            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
        }
        _ => return None,
    })
}

/// An HCL quoted string that is text only (no interpolation), as that text.
pub(crate) fn hcl_decode(lit: &str) -> Option<String> {
    hcl_pieces(lit)?
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Some(t),
            Piece::Expr(_) => None,
        })
        .collect()
}

/// A Ruby double-quoted string: `\\`, `\"` and `\#` are escapes; a bare `"` or an unescaped
/// `#{`, `#$` or `#@` (an interpolation) is refused.
pub(crate) fn rb_decode(lit: &str) -> Option<String> {
    let inner = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                e @ ('\\' | '"' | '#') => out.push(e),
                _ => return None,
            },
            '"' => return None,
            '#' if matches!(chars.peek(), Some('{' | '$' | '@')) => return None,
            plain => out.push(plain),
        }
    }
    Some(out)
}

fn is_ps_quote(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}')
}

/// A PowerShell single-quoted string: any single-quote character doubled stands for itself; a
/// lone one ends the string, so it is refused inside.
pub(crate) fn ps_decode(lit: &str) -> Option<String> {
    let inner = lit.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if is_ps_quote(c) && !is_ps_quote(chars.next()?) {
            return None;
        }
        out.push(c);
    }
    Some(out)
}
