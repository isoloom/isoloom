//! The literals the builders write, and decoders that read each back as its language would: a
//! decoder gives `None` for a literal the language would not read back as plain text (it ends
//! early, or something in it would expand). The quoting properties (`isoloom-targets`'
//! `tests/targets/quoting.rs`) and the fuzz target (`fuzz/fuzz_targets/quoting.rs`) both check
//! that every literal reads back as its text; HCL's own is [`crate::hcl_decode`].

/// A POSIX sh word made of single-quoted runs and backslash-escaped characters.
#[must_use]
pub fn sh_decode(word: &str) -> Option<String> {
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
#[must_use]
pub fn rb_decode(lit: &str) -> Option<String> {
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
#[must_use]
pub fn ps_decode(lit: &str) -> Option<String> {
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
#[must_use]
pub fn systemd_decode(word: &str) -> Option<String> {
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
#[must_use]
pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each `(literal, what it reads as)`: `None` where the language would expand or end it.
    fn reads(decode: fn(&str) -> Option<String>, cases: &[(&str, Option<&str>)]) {
        for (literal, text) in cases {
            assert_eq!(decode(literal).as_deref(), *text, "{literal}");
        }
    }

    #[test]
    fn a_sh_word_reads_as_its_quoted_runs_and_escapes_and_nothing_bare() {
        reads(
            sh_decode,
            &[
                ("'it'\\''s'", Some("it's")),
                ("'$HOME `x`'", Some("$HOME `x`")),
                ("\\$", Some("$")),
                ("''", Some("")),
                ("$HOME", None),
                ("'open", None),
                ("'a' b", None),
                ("trailing\\", None),
            ],
        );
    }

    #[test]
    fn a_ruby_string_reads_its_escapes_and_refuses_an_interpolation() {
        reads(
            rb_decode,
            &[
                (r#""a \"b\" \\ \#{x}""#, Some(r#"a "b" \ #{x}"#)),
                (r##""# a #""##, Some("# a #")),
                (r##""#{x}""##, None),
                (r##""#$x""##, None),
                (r##""#@y""##, None),
                (r#""a"b""#, None),
                (r#""\n""#, None),
                ("'single'", None),
            ],
        );
    }

    #[test]
    fn a_powershell_string_reads_a_doubled_quote_of_any_kind_as_one() {
        reads(
            ps_decode,
            &[
                ("'it''s $x'", Some("it's $x")),
                ("'a\u{2019}\u{2019}b'", Some("a\u{2019}b")),
                ("'it's'", None),
                ("'a\u{2018}b'", None),
                ("\"double\"", None),
            ],
        );
    }

    #[test]
    fn a_systemd_word_reads_its_escapes_and_doubled_specifiers_and_refuses_lone_ones() {
        reads(
            systemd_decode,
            &[
                (r#""a \"b\" \\ \n %% $$""#, Some("a \"b\" \\ \n % $")),
                (r#""%h""#, None),
                (r#""$HOME""#, None),
                (r#""a"b""#, None),
                (r#""\t""#, None),
                ("bare", None),
            ],
        );
    }

    #[test]
    fn base64_reads_as_rfc_4648_writes_it_and_refuses_anything_else() {
        for (text, bytes) in [("", &b""[..]), ("TQ==", b"M"), ("TWE=", b"Ma"), ("TWFu", b"Man"), ("+/+/", b"\xfb\xff\xbf")] {
            assert_eq!(b64_decode(text).as_deref(), Some(bytes), "{text}");
        }
        for wrong in ["TQ=", "TW!u", "TWFu\n"] {
            assert_eq!(b64_decode(wrong), None, "{wrong:?}");
        }
    }
}
