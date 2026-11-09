//! The literals the builders write, and decoders that read each back as its language would: a
//! decoder gives `None` for a literal the language would not read back as plain text (it ends
//! early, or something in it would expand). The quoting properties (`tests/quoting.rs`) and the fuzz
//! target (`fuzz/fuzz_targets/quoting.rs`) both check that every literal reads back as its text.

#[path = "hcl.rs"]
mod hcl;

pub(crate) use hcl::hcl_decode;

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

/// A word of a systemd command line in double quotes, as systemd reads it: C escapes (`\\`,
/// `\"`, `\n`) unescaped, `%%` a percent sign, `$$` a dollar sign; a lone `%` (a specifier) or
/// `$` (a variable) would expand, so it isn't plain text.
pub(crate) fn systemd_decode(word: &str) -> Option<String> {
    let inner = word.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(match chars.next()? {
                'n' => '\n',
                escaped @ ('\\' | '"') => escaped,
                _ => return None,
            }),
            '%' | '$' => {
                if chars.next()? != c {
                    return None;
                }
                out.push(c);
            }
            '"' => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}

/// Standard base64 (RFC 4648), padded, decoded.
pub(crate) fn b64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let (mut bits, mut held, mut out) = (0u32, 0u32, Vec::new());
    for symbol in text.trim_end_matches('=').bytes() {
        let sextet = ALPHABET.iter().position(|a| *a == symbol)?;
        bits = (bits << 6) | u32::try_from(sextet).ok()?;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push(u8::try_from((bits >> held) & 0xff).ok()?);
        }
    }
    Some(out)
}
