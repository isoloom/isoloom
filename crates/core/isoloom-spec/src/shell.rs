//! Shell quoting for the scripts Isoloom writes and runs.

/// `s` as one single-quoted POSIX sh word: `it's` becomes `'it'\''s'`, `""` becomes `''`.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Standard base64 (RFC 4648), padded, on one line: how a value of any bytes crosses a
/// script's standard input as one line of text.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64, quote};

    #[test]
    fn base64_is_the_standard_one() {
        for (raw, enc) in [
            ("", ""),
            ("a", "YQ=="),
            ("ab", "YWI="),
            ("abc", "YWJj"),
            ("abcd", "YWJjZA=="),
            ("abcde", "YWJjZGU="),
            ("abcdef", "YWJjZGVm"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn plain_word_is_wrapped() {
        assert_eq!(quote("abc"), "'abc'");
    }

    #[test]
    fn empty_string_is_an_empty_word() {
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn single_quotes_are_closed_escaped_and_reopened() {
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote("'"), "''\\'''");
    }

    #[test]
    fn shell_metacharacters_stay_literal() {
        assert_eq!(quote("$(rm -rf /) `x` \"y\" \\n"), "'$(rm -rf /) `x` \"y\" \\n'");
    }
}
