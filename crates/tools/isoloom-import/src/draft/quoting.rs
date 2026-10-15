//! Text from a source written into a draft so that the spec reads back exactly that text: a
//! scalar plain when nothing in it means something to YAML, else double-quoted with escapes; a
//! comment that stays a comment whatever line breaks the text holds.

/// The words YAML reads as something other than text when written plain.
const NOT_TEXT: [&str; 10] = ["true", "false", "null", "yes", "no", "on", "off", "y", "n", "~"];

/// Whether YAML reads `c` as a line break (YAML 1.1's set, which the parser follows).
fn line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Whether `c` may stand as itself in a YAML document (the `c-printable` set, without the line
/// breaks and the byte order mark).
fn printable(c: char) -> bool {
    matches!(c, '\t' | ' '..='~' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..) && !line_break(c) && c != '\u{feff}'
}

/// `text` as a double-quoted YAML scalar: quotes and backslashes escaped, and every character
/// that can't stand as itself (a line break, a control character) written as its escape.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if printable(c) => out.push(c),
            c => out.push_str(&escape(c)),
        }
    }
    out.push('"');
    out
}

/// `c` as a double-quoted scalar's escape: `\u` and four hex digits, `\U` and eight beyond.
fn escape(c: char) -> String {
    match u32::from(c) {
        code @ 0..=0xffff => format!("\\u{code:04x}"),
        code => format!("\\U{code:08x}"),
    }
}

/// `text` as a YAML scalar: as it is when YAML reads it back as that same text in any place
/// (block or flow), else [`quoted`].
pub(crate) fn scalar(text: &str) -> String {
    let plain = text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '_')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-' | '@' | '+'))
        && !NOT_TEXT.contains(&text.to_ascii_lowercase().as_str());
    if plain { text.to_owned() } else { quoted(text) }
}

/// `text` as YAML comment lines, `# ` before each: a line break in it starts another comment
/// line, and a character that can't stand in a document becomes `?`.
pub(crate) fn comment(text: &str) -> String {
    text.split(line_break)
        .map(|line| {
            let shown: String = line.chars().map(|c| if printable(c) { c } else { '?' }).collect();
            format!("# {shown}").trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// That YAML reads `text` back from `key: <written>`, in a block and in a flow mapping.
    fn reads_back(written: &str, text: &str) -> Result<(), TestCaseError> {
        for doc in [format!("key: {written}\n"), format!("{{ key: {written}, other: x }}\n")] {
            let read: std::collections::BTreeMap<String, String> = serde_yaml_ng::from_str(&doc).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(read.get("key").map(String::as_str), Some(text), "{}", doc);
        }
        Ok(())
    }

    #[test]
    fn a_plain_path_stays_plain_and_anything_else_is_quoted() {
        assert_eq!(scalar("provision/web.sh"), "provision/web.sh");
        assert_eq!(scalar("/var/lib/data"), "/var/lib/data");
        assert_eq!(scalar("yes"), "\"yes\"");
        assert_eq!(scalar("8080"), "\"8080\"");
        assert_eq!(scalar("my data"), "\"my data\"");
        assert_eq!(quoted("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn a_comment_with_line_breaks_is_comment_lines() {
        assert_eq!(comment("one\ntwo\rthree"), "# one\n# two\n# three");
        assert_eq!(comment("a\u{1}b"), "# a?b");
    }

    proptest! {
        #[test]
        fn quoted_text_reads_back_as_itself(text in any::<String>()) {
            reads_back(&quoted(&text), &text)?;
        }

        #[test]
        fn a_scalar_reads_back_as_itself(text in "[ -~]{1,30}") {
            reads_back(&scalar(&text), &text)?;
        }

        #[test]
        fn a_comment_stays_a_comment(text in any::<String>()) {
            let doc = format!("{}\nkey: x\n", comment(&text));
            let read: std::collections::BTreeMap<String, String> = serde_yaml_ng::from_str(&doc).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(read.len(), 1);
        }
    }
}
