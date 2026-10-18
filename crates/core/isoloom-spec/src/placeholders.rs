//! `{{ path }}` placeholders in a text: the message's (`{{ machines.web.ports.site.published }}`)
//! and a cloud module's inputs (`{{ inputs.REGION }}`). One reading of the syntax for every
//! text that holds them.

/// A piece of a text that may hold `{{ path }}` placeholders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Piece<'a> {
    /// Text as written.
    Text(&'a str),
    /// A placeholder: what is between its braces, as written (`path` trims it).
    Placeholder(&'a str),
    /// A `{{` without its `}}`, and the text after the `{{`.
    Unclosed(&'a str),
}

/// The pieces of `text`, in order: an unclosed `{{` ends it.
pub(crate) fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{{") {
        if !before.is_empty() {
            out.push(Piece::Text(before));
        }
        let Some((inner, next)) = after.split_once("}}") else {
            out.push(Piece::Unclosed(after));
            return out;
        };
        out.push(Piece::Placeholder(inner));
        rest = next;
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_placeholders_alternate() {
        assert_eq!(
            pieces("at {{ machines.web.ports.site.url }}, or {{x}}"),
            [
                Piece::Text("at "),
                Piece::Placeholder(" machines.web.ports.site.url "),
                Piece::Text(", or "),
                Piece::Placeholder("x")
            ]
        );
        assert_eq!(pieces("{{}}{{ a }}"), [Piece::Placeholder(""), Piece::Placeholder(" a ")]);
        assert_eq!(pieces(""), []);
        assert_eq!(pieces("plain } text {"), [Piece::Text("plain } text {")]);
    }

    #[test]
    fn an_unclosed_placeholder_ends_the_text() {
        assert_eq!(
            pieces("a {{ b }} c {{ d"),
            [Piece::Text("a "), Piece::Placeholder(" b "), Piece::Text(" c "), Piece::Unclosed(" d")]
        );
    }
}
