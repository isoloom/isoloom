//! The builders' literals, as properties: each reads back as its text (round trip) and nothing
//! in it can end the literal early or expand (injection safety): shell, Ruby and PowerShell
//! strings, and the HCL builder's strings and templates, whose only interpolations are the
//! builder's own. The decoders (`quoting/literals.rs`, which the fuzz target shares) follow each
//! language's own rules for that kind of literal; HCL's structure is read back by `hcl-rs`'s
//! parser.

use hcl::Expression;
use isoloom_render::hcl_builder::{Block, Module, Template, list, object, reference, string};
use isoloom_render::{powershell, ruby};
use isoloom_spec::shell::quote;
use proptest::prelude::*;

#[path = "quoting/literals.rs"]
mod literals;

use literals::{Piece, hcl_decode, hcl_pieces, hcl_string, ps_decode, rb_decode, sh_decode};

/// Characters that matter to some quoting rule, mixed with ordinary text.
fn tricky() -> impl Strategy<Value = String> {
    let special = prop::sample::select(vec![
        "'", "\"", "\\", "$", "{", "}", "%", "#", "@", "`", "\n", "\t", "\r", " ", ";", "|", "&", "(", ")", "${", "%{", "$${", "#{", "#$", "#@", "\u{1}",
        "\u{2018}", "\u{2019}", "\u{201A}", "\u{201B}", "$(", "é", "a", "Z", "0",
    ]);
    prop::collection::vec(prop_oneof![special.prop_map(str::to_owned), "[ -~]{0,4}"], 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn shell_quote_round_trips(s in tricky()) {
        prop_assert_eq!(sh_decode(&quote(&s)), Some(s));
    }

    #[test]
    fn an_hcl_string_round_trips(s in tricky()) {
        let lit = hcl_string(&s);
        prop_assert_eq!(lit.as_deref().and_then(hcl_decode), Some(s), "{:?}", lit);
    }

    #[test]
    fn a_ruby_string_round_trips(s in tricky()) {
        prop_assert_eq!(rb_decode(&ruby::quote(&s)), Some(s));
    }

    #[test]
    fn a_powershell_string_round_trips(s in tricky()) {
        prop_assert_eq!(ps_decode(&powershell::quote(&s)), Some(s));
    }
}

/// The real shell agrees with [`sh_decode`]: `printf %s <quoted>` prints the input back.
#[cfg(unix)]
mod real_shell {
    use super::{quote, tricky};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn sh_prints_the_input_back(s in tricky()) {
            let out = std::process::Command::new("sh").arg("-c").arg(format!("printf %s {}", quote(&s))).output().expect("sh runs");
            prop_assert!(out.status.success());
            prop_assert_eq!(String::from_utf8(out.stdout).expect("UTF-8"), s);
        }
    }
}

/// The module with one block holding `value` as attribute `v`.
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
        prop_assert_eq!(quoted(&text).and_then(hcl_pieces), Some(text_only(&s)), "{}", text);
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
