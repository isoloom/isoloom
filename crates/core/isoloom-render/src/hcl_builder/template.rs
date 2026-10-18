//! Strings that hold Terraform expressions: text and interpolations, kept apart until written.

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
    #[must_use]
    pub fn new() -> Template {
        Template::default()
    }

    /// A template made of `text` only.
    #[must_use]
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
    #[must_use]
    pub fn is_text(&self) -> bool {
        self.parts.iter().all(|p| matches!(p, Part::Text(_)))
    }

    /// The template as an expression: a plain string when it has no interpolation, the
    /// expression itself when it is one interpolation and nothing else (`"${x}"` is a form
    /// Terraform deprecates for `x`, which it converts where a string is wanted).
    #[must_use]
    pub fn into_expression(mut self) -> Expression {
        if let [Part::Expr(_)] = self.parts.as_slice()
            && let Some(Part::Expr(e)) = self.parts.pop()
        {
            return e;
        }
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
        let next = self.parts.iter().skip(1).map(Some).chain([None]);
        let raw = self
            .parts
            .iter()
            .zip(next)
            .map(|(p, after)| match p {
                // A `$` right before an interpolation would make it `$${`, a literal: written as
                // its escape instead.
                Part::Text(t) => {
                    let text = quoted_inside(t);
                    if let Some(head) = text.strip_suffix('$').filter(|_| matches!(after, Some(Part::Expr(_)))) {
                        format!("{head}\\u0024")
                    } else {
                        text
                    }
                }
                // `hcl-rs` documents an expression's formatting as unable to fail.
                Part::Expr(e) => format!("${{{}}}", strings_escaped(e)),
            })
            .collect::<Vec<_>>()
            .concat();
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
    use super::{Piece, Template};
    use crate::hcl_builder::reference;

    #[test]
    fn text_is_escaped_and_expressions_interpolated() {
        let t = Template::text("echo \"${HOME}\" %{x}\n").with_expr(reference("var.slot")).with_text("!");
        assert_eq!(t.into_expression().to_string(), r#""echo \"$${HOME}\" %%{x}\n${var.slot}!""#);
    }

    #[test]
    fn adjacent_text_is_one_piece_and_empty_text_none() {
        let t = Template::text("a").with_text("b").with_text("").with_expr(reference("var.x")).with_text("");
        let pieces: Vec<Piece<'_>> = t.pieces().collect();
        assert_eq!(pieces, [Piece::Text("ab"), Piece::Expr(&reference("var.x"))]);
        assert_eq!(Template::new().with_text("").pieces().count(), 0);
        assert!(!t.is_text() && Template::text("x").is_text());
    }

    #[test]
    fn a_dollar_before_an_interpolation_stays_a_dollar() {
        let t = Template::text("cost: $").with_expr(reference("var.price"));
        assert_eq!(t.into_expression().to_string(), r#""cost: \u0024${var.price}""#);
    }

    #[test]
    fn appending_keeps_text_and_expressions_in_order() {
        let mut t = Template::text("a");
        t.append(Template::text("b").with_expr(reference("var.y")).with_text("c"));
        assert_eq!(t.into_expression().to_string(), r#""ab${var.y}c""#);
    }

    #[test]
    fn text_converts_into_a_template_of_it() {
        assert_eq!(Template::from("a").into_expression(), hcl::Expression::String("a".into()));
        assert_eq!(Template::from(String::from("b")).into_expression(), hcl::Expression::String("b".into()));
        assert_eq!(hcl::Expression::from(Template::text("c")), hcl::Expression::String("c".into()));
    }

    #[test]
    fn a_lone_interpolation_is_its_expression_not_a_string_around_it() {
        assert_eq!(Template::new().with_expr(reference("var.x")).into_expression(), reference("var.x"));
        assert_eq!(
            Template::new().with_expr(reference("var.x")).with_text("").into_expression(),
            reference("var.x")
        );
        assert_eq!(
            Template::text(" ").with_expr(reference("var.x")).into_expression().to_string(),
            r#"" ${var.x}""#
        );
    }

    #[test]
    fn plain_text_is_a_string() {
        assert_eq!(Template::text("a").with_text("b").into_expression(), hcl::Expression::String("ab".into()));
    }
}
