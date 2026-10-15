//! Expression constructors: short names for the `hcl-rs` expressions a module is made of.

use hcl::Identifier;
use hcl::expr::{
    BinaryOp, BinaryOperator, Conditional, Expression, ForExpr, FuncCall, Object, ObjectKey, Traversal, TraversalOperator, UnaryOp, UnaryOperator, Variable,
};

/// A string value (escaped when written).
#[must_use]
pub fn string(s: impl Into<String>) -> Expression {
    Expression::String(s.into())
}

/// A whole number.
#[must_use]
pub fn number(n: u64) -> Expression {
    Expression::from(n)
}

/// `true` or `false`.
#[must_use]
pub fn boolean(b: bool) -> Expression {
    Expression::Bool(b)
}

/// `null`.
#[must_use]
pub fn null() -> Expression {
    Expression::Null
}

/// A reference by its dotted path: `var.node`, `aws_vpc.env.id`. Each part is an identifier
/// (resource names come from `res`, which keeps them so).
#[must_use]
pub fn reference(path: &str) -> Expression {
    let mut parts = path.split('.');
    let root = Variable::sanitized(parts.next().unwrap_or_default());
    let ops: Vec<TraversalOperator> = parts.map(|p| TraversalOperator::GetAttr(Identifier::sanitized(p))).collect();
    if ops.is_empty() {
        Expression::Variable(root)
    } else {
        Traversal::new(root, ops).into()
    }
}

/// `expr[i]`.
#[must_use]
pub fn index(expr: Expression, i: u64) -> Expression {
    Traversal::new(expr, [TraversalOperator::Index(number(i))]).into()
}

/// `expr.name`: an attribute of an expression that isn't a plain path (`list[0].id`).
#[must_use]
pub fn attr(expr: Expression, name: &str) -> Expression {
    Traversal::new(expr, [TraversalOperator::GetAttr(Identifier::sanitized(name))]).into()
}

/// A function call.
#[must_use]
pub fn call(name: &str, args: impl IntoIterator<Item = Expression>) -> Expression {
    let mut c = FuncCall::new(Identifier::sanitized(name));
    c.args = args.into_iter().collect();
    c.into()
}

/// A list.
#[must_use]
pub fn list(items: impl IntoIterator<Item = Expression>) -> Expression {
    Expression::Array(items.into_iter().collect())
}

/// Words a bare object key can't be: HCL reads them as a `for` expression or a literal.
const KEYWORDS: [&str; 6] = ["for", "if", "in", "null", "true", "false"];

/// An object, in the order given. A key that is an identifier is written bare, any other quoted.
#[must_use]
pub fn object<K: AsRef<str>>(items: impl IntoIterator<Item = (K, Expression)>) -> Expression {
    let object: Object<ObjectKey, Expression> = items
        .into_iter()
        .map(|(k, v)| {
            let k = k.as_ref();
            let key = match Identifier::new(k) {
                Ok(i) if !KEYWORDS.contains(&k) => ObjectKey::Identifier(i),
                _ => ObjectKey::Expression(string(k)),
            };
            (key, v)
        })
        .collect();
    Expression::Object(object)
}

/// `cond ? yes : no`.
#[must_use]
pub fn conditional(cond: Expression, yes: Expression, no: Expression) -> Expression {
    Conditional::new(cond, yes, no).into()
}

/// `lhs <op> rhs`.
#[must_use]
pub fn binary(lhs: Expression, op: BinaryOperator, rhs: Expression) -> Expression {
    BinaryOp::new(lhs, op, rhs).into()
}

/// `!expr`.
#[must_use]
pub fn not(expr: Expression) -> Expression {
    UnaryOp::new(UnaryOperator::Not, expr).into()
}

/// `[for <var> in <collection> : <value> if <cond>]`.
#[must_use]
pub fn for_list(var: &str, collection: Expression, value: Expression, cond: Option<Expression>) -> Expression {
    let f = ForExpr::new(Identifier::sanitized(var), collection, value);
    match cond {
        Some(c) => f.with_cond_expr(c).into(),
        None => f.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcl_builder::write::compact;

    /// An expression on one line, as a module writes a short one.
    fn written(e: &Expression) -> String {
        compact(e).expect("written")
    }

    #[test]
    fn each_value_writes_its_hcl() {
        assert_eq!(written(&string("a")), "\"a\"");
        assert_eq!(written(&number(7)), "7");
        assert_eq!((written(&boolean(true)), written(&boolean(false))), ("true".to_owned(), "false".to_owned()));
        assert_eq!(written(&null()), "null");
        assert_eq!(written(&list([number(1), string("b")])), "[1, \"b\"]");
    }

    #[test]
    fn each_reference_writes_its_path() {
        assert_eq!(written(&reference("var")), "var");
        assert_eq!(written(&reference("aws_vpc.env.id")), "aws_vpc.env.id");
        assert_eq!(written(&index(reference("x.list"), 2)), "x.list[2]");
        assert_eq!(written(&attr(index(reference("x.list"), 0), "id")), "x.list[0].id");
    }

    #[test]
    fn each_operation_writes_its_hcl() {
        assert_eq!(written(&call("max", [number(1), number(2)])), "max(1, 2)");
        assert_eq!(written(&conditional(reference("c"), number(1), number(2))), "c ? 1 : 2");
        assert_eq!(written(&binary(reference("a"), BinaryOperator::Eq, number(1))), "a == 1");
        assert_eq!(written(&not(reference("a"))), "!a");
        assert_eq!(written(&for_list("x", reference("xs"), reference("x"), None)), "[for x in xs : x]");
        assert_eq!(
            written(&for_list("x", reference("xs"), reference("x"), Some(reference("x.ok")))),
            "[for x in xs : x if x.ok]"
        );
    }

    #[test]
    fn an_object_key_that_isnt_a_plain_identifier_is_quoted() {
        let o = object([("web", number(1)), ("in", number(2)), ("a-b", number(3)), ("1x", number(4)), ("a.b", number(5))]);
        assert_eq!(written(&o), "{ web = 1, \"in\" = 2, a-b = 3, \"1x\" = 4, \"a.b\" = 5 }");
    }
}
