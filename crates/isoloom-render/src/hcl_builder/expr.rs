//! Expression constructors: short names for the `hcl-rs` expressions a module is made of.

use hcl::Identifier;
use hcl::expr::{
    BinaryOp, BinaryOperator, Conditional, Expression, ForExpr, FuncCall, Object, ObjectKey, Traversal, TraversalOperator, UnaryOp, UnaryOperator, Variable,
};

/// A string value (escaped when written).
pub fn string(s: impl Into<String>) -> Expression {
    Expression::String(s.into())
}

/// A whole number.
pub fn number(n: u64) -> Expression {
    Expression::from(n)
}

/// `true` or `false`.
pub fn boolean(b: bool) -> Expression {
    Expression::Bool(b)
}

/// `null`.
pub fn null() -> Expression {
    Expression::Null
}

/// A reference by its dotted path: `var.node`, `aws_vpc.env.id`. Each part is an identifier
/// (resource names come from `res`, which keeps them so).
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
pub fn index(expr: Expression, i: u64) -> Expression {
    Traversal::new(expr, [TraversalOperator::Index(number(i))]).into()
}

/// A function call.
pub fn call(name: &str, args: impl IntoIterator<Item = Expression>) -> Expression {
    let mut c = FuncCall::new(Identifier::sanitized(name));
    c.args = args.into_iter().collect();
    c.into()
}

/// A list.
pub fn list(items: impl IntoIterator<Item = Expression>) -> Expression {
    Expression::Array(items.into_iter().collect())
}

/// Words a bare object key can't be: HCL reads them as a `for` expression or a literal.
const KEYWORDS: [&str; 6] = ["for", "if", "in", "null", "true", "false"];

/// An object, in the order given. A key that is an identifier is written bare, any other quoted.
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
pub fn conditional(cond: Expression, yes: Expression, no: Expression) -> Expression {
    Conditional::new(cond, yes, no).into()
}

/// `lhs <op> rhs`.
pub fn binary(lhs: Expression, op: BinaryOperator, rhs: Expression) -> Expression {
    BinaryOp::new(lhs, op, rhs).into()
}

/// `!expr`.
pub fn not(expr: Expression) -> Expression {
    UnaryOp::new(UnaryOperator::Not, expr).into()
}

/// `[for <var> in <collection> : <value> if <cond>]`.
pub fn for_list(var: &str, collection: Expression, value: Expression, cond: Option<Expression>) -> Expression {
    let f = ForExpr::new(Identifier::sanitized(var), collection, value);
    match cond {
        Some(c) => f.with_cond_expr(c).into(),
        None => f.into(),
    }
}
