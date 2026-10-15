//! The quoting helpers, as properties: each literal decodes back to its input (round trip) and
//! nothing in it can end the literal early or expand (injection safety). The decoders
//! (`quoting/decode.rs`, which the fuzz targets share) follow each language's own rules for that
//! kind of literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_render::{hcl, ps, rb};
use isoloom_spec::shell::quote;
use proptest::prelude::*;

#[path = "quoting/decode.rs"]
mod decode;

use decode::{hcl_decode, ps_decode, rb_decode, sh_decode};

/// Characters that matter to some quoting rule, mixed with ordinary text.
fn tricky() -> impl Strategy<Value = String> {
    let special = prop::sample::select(vec![
        "'", "\"", "\\", "$", "{", "}", "%", "#", "@", "`", "\n", "\t", " ", ";", "|", "&", "(", ")", "${", "%{", "#{", "#$", "#@", "\u{2018}", "\u{2019}",
        "\u{201A}", "\u{201B}", "$(", "é", "a", "Z", "0",
    ]);
    prop::collection::vec(prop_oneof![special.prop_map(str::to_string), "[ -~]{0,4}"], 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn shell_quote_round_trips(s in tricky()) {
        prop_assert_eq!(sh_decode(&quote(&s)), Some(s));
    }

    #[test]
    fn hcl_round_trips(s in tricky()) {
        prop_assert_eq!(hcl_decode(&hcl(&s)), Some(s));
    }

    #[test]
    fn rb_round_trips(s in tricky()) {
        prop_assert_eq!(rb_decode(&rb(&s)), Some(s));
    }

    #[test]
    fn ps_round_trips(s in tricky()) {
        prop_assert_eq!(ps_decode(&ps(&s)), Some(s));
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
            let out = std::process::Command::new("sh").arg("-c").arg(format!("printf %s {}", quote(&s))).output().unwrap();
            prop_assert!(out.status.success());
            prop_assert_eq!(String::from_utf8(out.stdout).unwrap(), s);
        }
    }
}
