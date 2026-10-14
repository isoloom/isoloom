//! Strings that hold Terraform expressions: text and interpolations, kept apart until written.

use std::fmt::Write as _;

use hcl::expr::{Expression, TemplateExpr};

use super::escape::{quoted_inside, strings_escaped};

/// A quoted HCL template: text, escaped when written, and expressions, interpolated as
/// `${...}`. Text can never become an interpolation, whatever it holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Template {
    parts: Vec<Part>,
}

/// A piece of a template: text, or an interpolated expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece<'a> {
    /// Text, as it reads (unescaped).
    Text(&'a str),
    /// An expression.
    Expr(&'a Expression),
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    Expr(Expression),
}

impl Template {
    /// An empty template.
    pub fn new() -> Template {
        Template::default()
    }

    /// A template made of `text` only.
    pub fn text(text: impl Into<String>) -> Template {
        let mut t = Template::new();
        t.push_text(text);
        t
    }

    /// The template with `text` appended.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Template {
        self.push_text(text);
        self
    }

    /// The template with `expr` interpolated at its end.
    #[must_use]
    pub fn with_expr(mut self, expr: Expression) -> Template {
        self.push_expr(expr);
        self
    }

    /// Appends `text`.
    pub fn push_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        match self.parts.last_mut() {
            Some(Part::Text(t)) => t.push_str(&text),
            _ if text.is_empty() => {}
            _ => self.parts.push(Part::Text(text)),
        }
    }

    /// Appends an interpolation of `expr`.
    pub fn push_expr(&mut self, expr: Expression) {
        self.parts.push(Part::Expr(expr));
    }

    /// Appends `other`.
    pub fn append(&mut self, other: Template) {
        for p in other.parts {
            match p {
                Part::Text(t) => self.push_text(t),
                Part::Expr(e) => self.push_expr(e),
            }
        }
    }

    /// Its text and expressions, in order (for a caller that writes the template another way).
    pub fn pieces(&self) -> impl Iterator<Item = Piece<'_>> {
        self.parts.iter().map(|p| match p {
            Part::Text(t) => Piece::Text(t),
            Part::Expr(e) => Piece::Expr(e),
        })
    }

    /// Whether it is plain text (no interpolation).
    pub fn is_text(&self) -> bool {
        self.parts.iter().all(|p| matches!(p, Part::Text(_)))
    }

    /// The template as an expression: a plain string when it has no interpolation.
    pub fn into_expression(self) -> Expression {
        if self.is_text() {
            let text: String = self
                .parts
                .into_iter()
                .filter_map(|p| match p {
                    Part::Text(t) => Some(t),
                    Part::Expr(_) => None,
                })
                .collect();
            return Expression::String(text);
        }
        let mut raw = String::new();
        for (i, p) in self.parts.iter().enumerate() {
            match p {
                Part::Text(t) => {
                    let mut text = quoted_inside(t);
                    // A `$` right before an interpolation would make it `$${`, a literal: written
                    // as its escape instead.
                    if matches!(self.parts.get(i + 1), Some(Part::Expr(_))) && text.ends_with('$') {
                        text.pop();
                        text.push_str("\\u0024");
                    }
                    raw.push_str(&text);
                }
                Part::Expr(e) => {
                    let _ = write!(raw, "${{{}}}", strings_escaped(e));
                }
            }
        }
        Expression::TemplateExpr(Box::new(TemplateExpr::QuotedString(raw)))
    }
}

impl From<&str> for Template {
    fn from(text: &str) -> Template {
        Template::text(text)
    }
}

impl From<String> for Template {
    fn from(text: String) -> Template {
        Template::text(text)
    }
}

impl From<Template> for Expression {
    fn from(t: Template) -> Expression {
        t.into_expression()
    }
}

#[cfg(test)]
mod tests {
    use super::Template;
    use crate::hcl_builder::reference;

    #[test]
    fn text_is_escaped_and_expressions_interpolated() {
        let t = Template::text("echo \"${HOME}\" %{x}\n").with_expr(reference("var.slot")).with_text("!");
        assert_eq!(t.into_expression().to_string(), r#""echo \"$${HOME}\" %%{x}\n${var.slot}!""#);
    }

    #[test]
    fn plain_text_is_a_string() {
        assert_eq!(Template::text("a").with_text("b").into_expression(), hcl::Expression::String("ab".into()));
    }
}
