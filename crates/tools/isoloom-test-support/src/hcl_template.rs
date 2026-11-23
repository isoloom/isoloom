//! HCL's quoted templates read back as HCL's own template scanner reads them: shared by the
//! quoting properties (`isoloom-targets`' `tests/targets/quoting.rs`), the fuzz target, and the
//! HCL builder's own properties (`isoloom-terraform`'s `hcl_builder/tests.rs`), which read its
//! templates back with it.

/// One piece of an HCL quoted template: text, or an interpolated expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HclPiece {
    /// Text, as it reads.
    Text(String),
    /// The expression inside `${...}`, as written.
    Expr(String),
}

/// An HCL quoted template as its pieces, read as HCL's template scanner does, left to right:
/// `$${` and `%%{` are the literal markers, `${...}` an interpolation, a backslash an escape
/// (`\n`, `\r`, `\t`, `\\`, `\"`, `\uXXXX`). `None` for a bare `"`, a line break, a directive
/// (`%{`) or anything else HCL wouldn't read as one well-formed template.
#[must_use]
pub fn hcl_pieces(lit: &str) -> Option<Vec<HclPiece>> {
    let mut rest = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out: Vec<HclPiece> = Vec::new();
    let mut text = String::new();
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("$${").or_else(|| rest.strip_prefix("%%{")) {
            text.push(c);
            text.push('{');
            rest = after;
        } else if let Some(after) = rest.strip_prefix("${") {
            let (expr, tail) = after.split_once('}')?;
            out.extend((!text.is_empty()).then(|| HclPiece::Text(std::mem::take(&mut text))));
            out.push(HclPiece::Expr(expr.to_owned()));
            rest = tail;
        } else if rest.starts_with("%{") || c == '"' || c == '\n' {
            return None;
        } else {
            let mut chars = rest.chars();
            chars.next();
            if c == '\\' {
                text.push(escape(&mut chars)?);
            } else {
                text.push(c);
            }
            rest = chars.as_str();
        }
    }
    out.extend((!text.is_empty()).then_some(HclPiece::Text(text)));
    Some(out)
}

/// The character an HCL escape stands for, `chars` just after its backslash.
fn escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
    Some(match chars.next()? {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '"' => '"',
        'u' => {
            // Exactly four hex digits: HCL refuses a shorter escape, or a sign.
            let hex: String = chars.by_ref().take(4).collect();
            if hex.len() != 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
        }
        _ => return None,
    })
}

/// An HCL quoted string that is text only (no interpolation), as that text.
#[must_use]
pub fn hcl_decode(lit: &str) -> Option<String> {
    hcl_pieces(lit)?
        .into_iter()
        .map(|p| match p {
            HclPiece::Text(t) => Some(t),
            HclPiece::Expr(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_reads_as_its_text_and_interpolations_its_markers_and_escapes_literal() {
        let text = |t: &str| HclPiece::Text(t.to_owned());
        let expr = |e: &str| HclPiece::Expr(e.to_owned());
        assert_eq!(hcl_pieces(r#""a ${var.x} b""#), Some(vec![text("a "), expr("var.x"), text(" b")]));
        assert_eq!(hcl_pieces(r#""$${x} %%{y} \"q\" \\ \té""#), Some(vec![text("${x} %{y} \"q\" \\ \té")]));
        assert_eq!(hcl_pieces(r#""""#), Some(vec![]));
        for wrong in [
            r#""%{ if x }""#,
            r#""a"b""#,
            "\"a\nb\"",
            r#""${open""#,
            r#""\q""#,
            r#""\u00""#,
            r#""\u+0e9""#,
            "unquoted",
        ] {
            assert_eq!(hcl_pieces(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn a_string_is_text_only_when_it_interpolates_nothing() {
        assert_eq!(hcl_decode(r#""plain $${x}""#).as_deref(), Some("plain ${x}"));
        assert_eq!(hcl_decode(r#""a ${var.x}""#), None);
        assert_eq!(hcl_decode(r#""\q""#), None);
    }
}
