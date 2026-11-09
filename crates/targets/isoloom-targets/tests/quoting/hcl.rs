//! HCL's quoted templates read back as HCL's own template scanner reads them: shared by the
//! quoting properties (`literals.rs`), the fuzz target, and the HCL builder's own properties
//! (`isoloom-terraform`'s `hcl_builder/tests.rs`), which read its templates back with it.

/// One piece of an HCL quoted template: text, or an interpolated expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// Text, as it reads.
    Text(String),
    /// The expression inside `${...}`, as written.
    Expr(String),
}

/// An HCL quoted template as its pieces, read as HCL's template scanner does, left to right:
/// `$${` and `%%{` are the literal markers, `${...}` an interpolation, a backslash an escape
/// (`\n`, `\r`, `\t`, `\\`, `\"`, `\uXXXX`). `None` for a bare `"`, a line break, a directive
/// (`%{`) or anything else HCL wouldn't read as one well-formed template.
pub(crate) fn hcl_pieces(lit: &str) -> Option<Vec<Piece>> {
    let mut rest = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("$${").or_else(|| rest.strip_prefix("%%{")) {
            text.push(c);
            text.push('{');
            rest = after;
        } else if let Some(after) = rest.strip_prefix("${") {
            let (expr, tail) = after.split_once('}')?;
            out.extend((!text.is_empty()).then(|| Piece::Text(std::mem::take(&mut text))));
            out.push(Piece::Expr(expr.to_owned()));
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
    out.extend((!text.is_empty()).then_some(Piece::Text(text)));
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
            let hex: String = chars.by_ref().take(4).collect();
            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
        }
        _ => return None,
    })
}

/// An HCL quoted string that is text only (no interpolation), as that text.
pub(crate) fn hcl_decode(lit: &str) -> Option<String> {
    hcl_pieces(lit)?
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Some(t),
            Piece::Expr(_) => None,
        })
        .collect()
}
