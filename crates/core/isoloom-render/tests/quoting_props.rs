//! The quoting helpers, as properties: each literal decodes back to its input (round trip) and
//! nothing in it can end the literal early or expand (injection safety). The decoders below
//! follow each language's own rules for that kind of literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_render::{hcl, ps, rb};
use isoloom_spec::shell::quote;
use proptest::prelude::*;

/// Characters that matter to some quoting rule, mixed with ordinary text.
fn tricky() -> impl Strategy<Value = String> {
    let special = prop::sample::select(vec![
        "'", "\"", "\\", "$", "{", "}", "%", "#", "@", "`", "\n", "\t", " ", ";", "|", "&", "(", ")", "${", "%{", "#{", "#$", "#@", "\u{2018}", "\u{2019}",
        "\u{201A}", "\u{201B}", "$(", "é", "a", "Z", "0",
    ]);
    prop::collection::vec(prop_oneof![special.prop_map(str::to_string), "[ -~]{0,4}"], 0..24).prop_map(|parts| parts.concat())
}

/// A POSIX sh word made of single-quoted runs and backslash-escaped characters.
fn sh_decode(word: &str) -> Option<String> {
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
fn hcl_decode(lit: &str) -> Option<String> {
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
fn rb_decode(lit: &str) -> Option<String> {
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
fn ps_decode(lit: &str) -> Option<String> {
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn shell_quote_round_trips(s in tricky()) {
        prop_assert_eq!(sh_decode(&quote(&s)), Some(s));
    }

    #[test]
    fn hcl_round_trips(s in tricky()) {
        prop_assert_eq!(hcl_decode(&hcl(&s)), Some(s));
    }

    #[test]
    fn rb_round_trips(s in tricky()) {
        prop_assert_eq!(rb_decode(&rb(&s)), Some(s));
    }

    #[test]
    fn ps_round_trips(s in tricky()) {
        prop_assert_eq!(ps_decode(&ps(&s)), Some(s));
    }
}

/// The real shell agrees with [`sh_decode`]: `printf %s <quoted>` prints the input back.
#[cfg(unix)]
mod real_shell {
    use super::{quote, tricky};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn sh_prints_the_input_back(s in tricky()) {
            let out = std::process::Command::new("sh").arg("-c").arg(format!("printf %s {}", quote(&s))).output().unwrap();
            prop_assert!(out.status.success());
            prop_assert_eq!(String::from_utf8(out.stdout).unwrap(), s);
        }
    }
}
