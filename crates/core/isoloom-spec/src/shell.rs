//! Shell quoting for the scripts Isoloom writes and runs.

/// `s` as one single-quoted POSIX sh word: `it's` becomes `'it'\''s'`, `""` becomes `''`.
#[must_use]
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Standard base64 (RFC 4648), padded, on one line: how a value of any bytes crosses a
/// script's standard input as one line of text.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|i| chunk.get(i).copied().unwrap_or(0));
        let sextets = [a >> 2, ((a & 0b11) << 4) | (b >> 4), ((b & 0b1111) << 2) | (c >> 6), c & 0b11_1111];
        // `n` bytes give `n + 1` symbols; padding fills the four.
        for (i, sextet) in sextets.into_iter().enumerate() {
            out.push(if i <= chunk.len() { symbol(sextet) } else { '=' });
        }
    }
    out
}

/// The base64 symbol of six bits.
fn symbol(sextet: u8) -> char {
    char::from(match sextet {
        0..=25 => b'A' + sextet,
        26..=51 => b'a' + (sextet - 26),
        52..=61 => b'0' + (sextet - 52),
        62 => b'+',
        _ => b'/',
    })
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
    fn base64_uses_every_symbol_of_its_alphabet() {
        let bytes = [
            0x00, 0x10, 0x83, 0x10, 0x51, 0x87, 0x20, 0x92, 0x8b, 0x30, 0xd3, 0x8f, 0x41, 0x14, 0x93, 0x51, 0x55, 0x97, 0x61, 0x96, 0x9b, 0x71, 0xd7, 0x9f,
            0x82, 0x18, 0xa3, 0x92, 0x59, 0xa7, 0xa2, 0x9a, 0xab, 0xb2, 0xdb, 0xaf, 0xc3, 0x1c, 0xb3, 0xd3, 0x5d, 0xb7, 0xe3, 0x9e, 0xbb, 0xf3, 0xdf, 0xbf,
        ];
        assert_eq!(base64(&bytes), "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/");
    }

    proptest::proptest! {
        #[test]
        fn base64_is_four_symbols_per_three_bytes_padded_at_the_end(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..64)) {
            let encoded = base64(&bytes);
            proptest::prop_assert_eq!(encoded.len(), bytes.len().div_ceil(3) * 4);
            let padding = encoded.chars().rev().take_while(|c| *c == '=').count();
            proptest::prop_assert_eq!(padding, (3 - bytes.len() % 3) % 3);
            proptest::prop_assert!(encoded.trim_end_matches('=').chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/'));
        }
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
