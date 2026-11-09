//! Strings as HCL quoted templates, escaped here rather than by `hcl-rs`: its marker escaping
//! leaves an already escaped `$${` (or `%%{`) alone, so a value holding the text `$${` would
//! reach Terraform as `${`. HCL's template scanner reads `$${` and `%%{` as the literal markers,
//! left to right; every other character is itself, or a backslash escape.

use hcl::expr::{Expression, Object, ObjectKey, Operation, TemplateExpr, TraversalOperator};

/// The inside of a quoted string that reads as `s`: backslash escapes for quotes, backslashes
/// and control characters, and `${` / `%{` written `$${` / `%%{`.
pub(super) fn quoted_inside(s: &str) -> String {
    let next = s.chars().skip(1).map(Some).chain([None]);
    s.chars()
        .zip(next)
        .map(|(c, after)| match c {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            '\t' => "\\t".to_owned(),
            c if c.is_control() => format!("\\u{:04x}", u32::from(c)),
            '$' | '%' if after == Some('{') => format!("{c}{c}"),
            c => c.to_string(),
        })
        .collect::<Vec<_>>()
        .concat()
}

/// `e` with every string value turned into a quoted template escaped by [`quoted_inside`], so
/// `hcl-rs` writes it as it is.
pub(super) fn strings_escaped(e: &Expression) -> Expression {
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
    use hcl::expr::{BinaryOperator, Expression};

    use super::{quoted_inside, strings_escaped};
    use crate::hcl_builder::write::compact;
    use crate::hcl_builder::{binary, boolean, call, conditional, for_list, index, list, not, object, reference, string};

    #[test]
    fn markers_are_escaped_every_time() {
        assert_eq!(quoted_inside("${a} $${b} %{c} $ %"), "$${a} $$${b} %%{c} $ %");
        assert_eq!(quoted_inside("\"\\\n\t\u{1}"), "\\\"\\\\\\n\\t\\u0001");
    }

    #[test]
    fn a_string_is_escaped_wherever_it_stands() {
        let marker = || string("${x}");
        let tree = list([
            marker(),
            object([("k", marker())]),
            call("f", [marker()]),
            conditional(marker(), marker(), marker()),
            binary(marker(), BinaryOperator::Eq, marker()),
            not(marker()),
            for_list("v", marker(), marker(), Some(marker())),
            index(list([marker()]), 0),
            Expression::Parenthesis(Box::new(marker())),
        ]);
        let written = compact(&strings_escaped(&tree)).expect("written");
        assert_eq!(written.matches("$${x}").count(), 14, "{written}");
        assert!(!written.replace("$${", "").contains("${"), "an interpolation slipped through: {written}");
        assert_eq!(strings_escaped(&reference("var.a")), reference("var.a"));
        assert_eq!(strings_escaped(&boolean(true)), boolean(true));
        let key = compact(&strings_escaped(&object([("${k}", boolean(true))]))).expect("written");
        assert_eq!(key, "{ \"$${k}\" = true }");
    }
}
