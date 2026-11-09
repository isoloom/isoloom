//! The builder's literals and structures read back, as properties: a string as its text, a
//! template's text never as an interpolation (both by the decoder of HCL's template rules the
//! quoting properties and the fuzz target share), and long lists and objects, laid out across
//! lines, by `hcl-rs`'s parser as built.

use hcl::Expression;
use proptest::prelude::*;

use super::{Block, Module, Template, list, object, quote, reference, string};

#[path = "../../../../targets/isoloom-targets/tests/quoting/hcl.rs"]
mod literals;

use literals::{Piece, hcl_decode, hcl_pieces};

/// Characters that matter to HCL's quoting, mixed with ordinary text.
fn tricky() -> impl Strategy<Value = String> {
    let special = prop::sample::select(vec![
        "'", "\"", "\\", "$", "{", "}", "%", "#", "\n", "\t", "\r", " ", "${", "%{", "$${", "%%{", "\u{1}", "é", "a", "0",
    ]);
    prop::collection::vec(prop_oneof![special.prop_map(str::to_owned), "[ -~]{0,4}"], 0..24).prop_map(|parts| parts.concat())
}

/// The module with one block holding `value` as attribute `v`, as written.
fn written(value: Expression) -> Result<String, hcl::Error> {
    let mut m = Module::new("");
    m.block(Block::builder("b").add_attribute(("v", value)).build());
    m.render()
}

/// The quoted value of `v = "..."` on one line.
fn quoted(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.trim_start().strip_prefix("v = "))
}

/// `s` as the pieces it reads back as: one text piece, or none for empty text.
fn text_only(s: &str) -> Vec<Piece> {
    if s.is_empty() { Vec::new() } else { vec![Piece::Text(s.to_owned())] }
}

proptest! {
    #[test]
    fn a_string_comes_back_as_written(s in tricky()) {
        let text = written(string(s.clone()))?;
        let literal = quote(&s);
        prop_assert_eq!(quoted(&text), Some(literal.as_str()), "a module holds the string's literal");
        prop_assert_eq!(quoted(&text).and_then(hcl_decode), Some(s), "{}", text);
    }

    #[test]
    fn template_text_never_becomes_an_interpolation(a in tricky(), b in tricky()) {
        // Without text around it, the interpolation is written as its bare expression.
        prop_assume!(!a.is_empty() || !b.is_empty());
        let text = written(Template::text(a.clone()).with_expr(reference("var.slot")).with_text(b.clone()).into_expression())?;
        let mut expected = text_only(&a);
        expected.push(Piece::Expr("var.slot".into()));
        expected.extend(text_only(&b));
        prop_assert_eq!(quoted(&text).and_then(hcl_pieces), Some(expected), "{}", text);
    }

    #[test]
    fn long_lists_and_objects_parse_back(items in prop::collection::vec(tricky().prop_map(|s| s.replace(['$', '%'], "S")), 0..12)) {
        let value = object([
            ("list", list(items.iter().cloned().map(string))),
            ("nested", object(items.iter().enumerate().map(|(i, s)| (format!("k{i}"), string(s.clone()))))),
        ]);
        let text = written(value.clone())?;
        let body = hcl::parse(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
        let parsed = body.blocks().next().and_then(|b| b.body().attributes().next()).map(|a| a.expr().clone());
        prop_assert_eq!(parsed, Some(value), "{}", text);
    }
}
