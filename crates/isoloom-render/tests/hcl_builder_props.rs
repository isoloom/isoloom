//! The HCL builder, as properties: whatever text a value holds, the written module reads back
//! as the same text (round trip), and the only interpolations in it are the builder's own
//! (injection safety). Strings are read by the decoder below, which follows HCL's template
//! scanner (`$${` and `%%{` are the literal markers, scanned left to right); structure is read
//! back by `hcl-rs`'s parser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use hcl::Expression;
use isoloom_render::hcl_builder::{Block, Module, Template, list, object, reference, string};
use proptest::prelude::*;

/// Characters that matter to HCL's quoting, mixed with ordinary text.
fn tricky() -> impl Strategy<Value = String> {
    let special = prop::sample::select(vec![
        "\"", "\\", "$", "{", "}", "%", "${", "%{", "$${", "\n", "\t", "\r", "'", "#", "\u{1}", "é", "a", " ", "}\"", "\\n",
    ]);
    prop::collection::vec(prop_oneof![special.prop_map(str::to_string), "[ -~]{0,4}"], 0..24).prop_map(|parts| parts.concat())
}

/// The module with one block holding `value` as attribute `v`.
fn written(value: Expression) -> String {
    let mut m = Module::new("");
    m.block(Block::builder("b").add_attribute(("v", value)).build());
    m.render().unwrap()
}

/// The quoted value of `v = "..."` on one line.
fn quoted(text: &str) -> &str {
    let line = text.lines().find(|l| l.trim_start().starts_with("v = ")).unwrap();
    line.trim_start().strip_prefix("v = ").unwrap()
}

/// A quoted HCL template as its pieces: `(false, text)` and `(true, interpolated expression)`.
/// `None` when it isn't one well-formed quoted template on its line.
fn decode(lit: &str) -> Option<Vec<(bool, String)>> {
    let inner = lit.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = vec![(false, String::new())];
    let mut rest = inner;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("$${").or_else(|| rest.strip_prefix("%%{")) {
            text(&mut out).push_str(if rest.starts_with('$') { "${" } else { "%{" });
            rest = r;
        } else if let Some(r) = rest.strip_prefix("${") {
            let end = r.find('}')?;
            out.push((true, r[..end].to_string()));
            out.push((false, String::new()));
            rest = &r[end + 1..];
        } else if rest.starts_with("%{") || c == '"' || c == '\n' {
            return None;
        } else if c == '\\' {
            let (ch, r) = escape(&rest[1..])?;
            text(&mut out).push(ch);
            rest = r;
        } else {
            text(&mut out).push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Some(out.into_iter().filter(|(e, s)| *e || !s.is_empty()).collect())
}

fn text(out: &mut [(bool, String)]) -> &mut String {
    &mut out.last_mut().unwrap().1
}

/// One escape after a backslash: the character and what follows it.
fn escape(rest: &str) -> Option<(char, &str)> {
    let e = rest.chars().next()?;
    let after = &rest[e.len_utf8()..];
    Some(match e {
        'n' => ('\n', after),
        't' => ('\t', after),
        'r' => ('\r', after),
        '\\' => ('\\', after),
        '"' => ('"', after),
        'u' => (char::from_u32(u32::from_str_radix(after.get(..4)?, 16).ok()?)?, &after[4..]),
        _ => return None,
    })
}

fn text_only(s: &str) -> Vec<(bool, String)> {
    if s.is_empty() { vec![] } else { vec![(false, s.to_string())] }
}

proptest! {
    #[test]
    fn a_string_comes_back_as_written(s in tricky()) {
        let text = written(string(s.clone()));
        prop_assert_eq!(decode(quoted(&text)), Some(text_only(&s)), "{}", text);
    }

    #[test]
    fn template_text_never_becomes_an_interpolation(a in tricky(), b in tricky()) {
        let text = written(Template::text(a.clone()).with_expr(reference("var.slot")).with_text(b.clone()).into_expression());
        let mut expected = text_only(&a);
        expected.push((true, "var.slot".into()));
        expected.extend(text_only(&b));
        prop_assert_eq!(decode(quoted(&text)), Some(expected), "{}", text);
    }

    #[test]
    fn long_lists_and_objects_parse_back(items in prop::collection::vec(tricky().prop_map(|s| s.replace(['$', '%'], "S")), 0..12)) {
        let value = object([
            ("list", list(items.iter().cloned().map(string))),
            ("nested", object(items.iter().enumerate().map(|(i, s)| (format!("k{i}"), string(s.clone()))))),
        ]);
        let text = written(value.clone());
        let body = hcl::parse(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
        let parsed = body.blocks().next().unwrap().body().attributes().next().unwrap().expr().clone();
        prop_assert_eq!(parsed, value, "{}", text);
    }
}
