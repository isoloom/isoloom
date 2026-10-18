//! Bytes as text: hexadecimal for digests, base64 for PowerShell's encoded commands; and text
//! quoted for a POSIX shell or PowerShell.

/// Lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [b >> 4, b & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Standard base64, with padding.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().zip([16u32, 8, 0]).fold(0u32, |n, (b, shift)| n | (u32::from(*b) << shift));
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            let letter = usize::try_from((n >> shift) & 63).ok().and_then(|at| ABC.get(at));
            out.push(match letter {
                Some(l) if i <= chunk.len() => char::from(*l),
                _ => '=',
            });
        }
    }
    out
}

/// `s` as one POSIX shell word: single-quoted, each `'` closed, escaped and reopened.
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// `s` as a PowerShell single-quoted string: each quote doubled, the typographic single quotes
/// too, since PowerShell takes them as quotes.
pub(crate) fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A POSIX shell's reading of `quoted` as one word made of quoted parts and `\` escapes
    /// (enough for `sh_quote`'s output), or `None` for anything else.
    fn sh_unquote(quoted: &str) -> Option<String> {
        let (mut out, mut chars) = (String::new(), quoted.chars());
        while let Some(c) = chars.next() {
            match c {
                '\'' => loop {
                    match chars.next()? {
                        '\'' => break,
                        inner => out.push(inner),
                    }
                },
                '\\' => out.push(chars.next()?),
                _ => return None,
            }
        }
        Some(out)
    }

    /// PowerShell's reading of a single-quoted string, or `None` when `quoted` isn't one.
    fn ps_unquote(quoted: &str) -> Option<String> {
        let quote = |c: char| matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}');
        let inner: Vec<char> = quoted.strip_prefix('\'')?.strip_suffix('\'')?.chars().collect();
        let (mut out, mut i) = (String::new(), 0);
        while let Some(&c) = inner.get(i) {
            if quote(c) {
                // Inside, a quote only stands doubled.
                if !inner.get(i + 1).copied().is_some_and(quote) {
                    return None;
                }
                i += 1;
            }
            out.push(c);
            i += 1;
        }
        Some(out)
    }

    proptest! {
        #[test]
        fn a_shell_reads_back_exactly_what_was_quoted(s in "\\PC*") {
            prop_assert_eq!(sh_unquote(&sh_quote(&s)), Some(s));
        }

        #[test]
        fn powershell_reads_back_exactly_what_was_quoted(s in "\\PC*") {
            prop_assert_eq!(ps_unquote(&ps_quote(&s)), Some(s));
        }
    }

    #[test]
    fn quoting_closes_every_quote() {
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        assert_eq!(ps_quote("it\u{2019}s"), "'it\u{2019}\u{2019}s'");
    }

    #[test]
    fn base64_pads_as_the_standard_says() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Many"), "TWFueQ==");
    }

    #[test]
    fn hex_is_two_lowercase_digits_a_byte() {
        assert_eq!(hex(&[0, 0xab, 0x10]), "00ab10");
    }
}
