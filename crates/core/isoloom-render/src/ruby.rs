//! Ruby, as a Vagrantfile is written: string literals that hold any text.

/// A Ruby double-quoted string literal reading as `s`: backslashes and quotes escaped, and every
/// `#` that could start an interpolation (`#{...}`, `#$global`, `#@ivar`) made literal.
#[must_use]
pub fn quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace("#{", "\\#{")
        .replace("#$", "\\#$")
        .replace("#@", "\\#@");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quotes_backslashes_and_interpolations_are_escaped() {
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(quote("#{x} #$y #@z # {"), r#""\#{x} \#$y \#@z # {""#);
    }
}
