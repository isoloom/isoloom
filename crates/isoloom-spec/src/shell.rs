//! Shell quoting for the scripts Isoloom writes and runs.

/// `s` as one single-quoted POSIX sh word: `it's` becomes `'it'\''s'`, `""` becomes `''`.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::quote;

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
