//! Strings as HCL quoted templates, escaped here rather than by `hcl-rs`: its marker escaping
//! leaves an already escaped `$${` (or `%%{`) alone, so a value holding the text `$${` would
//! reach Terraform as `${`. HCL's template scanner reads `$${` and `%%{` as the literal markers,
//! left to right; every other character is itself, or a backslash escape.

use std::fmt::Write as _;

use hcl::expr::{Expression, Object, ObjectKey, Operation, TemplateExpr, TraversalOperator};

/// The inside of a quoted string that reads as `s`: backslash escapes for quotes, backslashes
/// and control characters, and `${` / `%{` written `$${` / `%%{`.
pub fn quoted_inside(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            '$' | '%' if chars.peek() == Some(&'{') => {
                out.push(c);
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// `e` with every string value turned into a quoted template escaped by [`quoted_inside`], so
/// `hcl-rs` writes it as it is.
pub fn strings_escaped(e: &Expression) -> Expression {
    match e {
        Expression::String(s) => quoted(s),
        Expression::Array(items) => Expression::Array(items.iter().map(strings_escaped).collect()),
        Expression::Object(o) => Expression::Object(o.iter().map(|(k, v)| (key(k), strings_escaped(v))).collect::<Object<ObjectKey, Expression>>()),
        Expression::FuncCall(c) => {
            let mut c = (**c).clone();
            c.args = c.args.iter().map(strings_escaped).collect();
            c.into()
        }
        Expression::Conditional(c) => {
            let mut c = (**c).clone();
            c.cond_expr = strings_escaped(&c.cond_expr);
            c.true_expr = strings_escaped(&c.true_expr);
            c.false_expr = strings_escaped(&c.false_expr);
            c.into()
        }
        Expression::Operation(o) => operation(o),
        Expression::ForExpr(f) => {
            let mut f = (**f).clone();
            f.collection_expr = strings_escaped(&f.collection_expr);
            f.key_expr = f.key_expr.as_ref().map(strings_escaped);
            f.value_expr = strings_escaped(&f.value_expr);
            f.cond_expr = f.cond_expr.as_ref().map(strings_escaped);
            f.into()
        }
        Expression::Traversal(t) => {
            let mut t = (**t).clone();
            t.expr = strings_escaped(&t.expr);
            for op in &mut t.operators {
                if let TraversalOperator::Index(i) = op {
                    *i = strings_escaped(i);
                }
            }
            t.into()
        }
        Expression::Parenthesis(inner) => Expression::Parenthesis(Box::new(strings_escaped(inner))),
        other => other.clone(),
    }
}

fn quoted(s: &str) -> Expression {
    Expression::TemplateExpr(Box::new(TemplateExpr::QuotedString(quoted_inside(s))))
}

fn key(k: &ObjectKey) -> ObjectKey {
    match k {
        ObjectKey::Expression(e) => ObjectKey::Expression(strings_escaped(e)),
        other => other.clone(),
    }
}

fn operation(o: &Operation) -> Expression {
    match o {
        Operation::Unary(u) => {
            let mut u = u.clone();
            u.expr = strings_escaped(&u.expr);
            u.into()
        }
        Operation::Binary(b) => {
            let mut b = b.clone();
            b.lhs_expr = strings_escaped(&b.lhs_expr);
            b.rhs_expr = strings_escaped(&b.rhs_expr);
            b.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::quoted_inside;

    #[test]
    fn markers_are_escaped_every_time() {
        assert_eq!(quoted_inside("${a} $${b} %{c} $ %"), "$${a} $$${b} %%{c} $ %");
        assert_eq!(quoted_inside("\"\\\n\t\u{1}"), "\\\"\\\\\\n\\t\\u0001");
    }
}
