//! Decoders for the quoting helpers' literals, each following its language's own rules for that
//! kind of literal: `None` when the literal is not one the language would read back as plain text
//! (it ends early, or something in it would expand). The quoting properties and the fuzz targets
//! (`fuzz/`) both check that every literal decodes back to its input.

/// A POSIX sh word made of single-quoted runs and backslash-escaped characters.
pub(crate) fn sh_decode(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => loop {
                match chars.next()? {
                    '\'' => break,
                    c => out.push(c),
                }
            },
            '\\' => out.push(chars.next()?),
            // Anything else outside quotes would be read by the shell: not a safe word.
            _ => return None,
        }
    }
    Some(out)
}

/// An HCL quoted template: escapes, and `$${` / `%%{` as the literal `${` / `%{`. A bare `"`,
/// a newline or an unescaped `${` / `%{` (an interpolation or a directive) is refused.
pub(crate) fn hcl_decode(lit: &str) -> Option<String> {
    let inner = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut rest = inner;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("$${").or_else(|| rest.strip_prefix("%%{")) {
            out.push_str(if rest.starts_with('$') { "${" } else { "%{" });
            rest = r;
            continue;
        }
        if rest.starts_with("${") || rest.starts_with("%{") || c == '"' || c == '\n' {
            return None;
        }
        rest = &rest[c.len_utf8()..];
        if c == '\\' {
            let e = rest.chars().next()?;
            rest = &rest[e.len_utf8()..];
            out.push(match e {
                'n' => '\n',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            });
        } else {
            out.push(c);
        }
    }
    Some(out)
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
            c => out.push(c),
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
        if is_ps_quote(c) {
            let next = chars.next()?;
            if !is_ps_quote(next) {
                return None;
            }
        }
        out.push(c);
    }
    Some(out)
}
