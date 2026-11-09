//! The builders' literals, as properties: each reads back as its text (round trip) and nothing
//! in it can end the literal early or expand (injection safety): shell, Ruby and PowerShell
//! strings, and the HCL builder's strings (its templates and structure are its own crate's
//! properties, `isoloom-terraform`'s `hcl_builder/tests.rs`). The decoders (`quoting/literals.rs`,
//! which the fuzz target shares) follow each language's own rules for that kind of literal.

use isoloom_render::powershell;
use isoloom_spec::shell::quote;
use isoloom_vagrant::ruby_quote;
use proptest::prelude::*;

#[path = "quoting/literals.rs"]
mod literals;

use literals::{b64_decode, hcl_decode, ps_decode, rb_decode, sh_decode, systemd_decode};

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
        let lit = isoloom_terraform::hcl_quote(&s);
        prop_assert_eq!(hcl_decode(&lit), Some(s), "{:?}", lit);
    }

    #[test]
    fn a_ruby_string_round_trips(s in tricky()) {
        prop_assert_eq!(rb_decode(&ruby_quote(&s)), Some(s));
    }

    #[test]
    fn a_powershell_string_round_trips(s in tricky()) {
        prop_assert_eq!(ps_decode(&powershell::quote(&s)), Some(s));
    }

    /// A service Isoloom defines: each word of its `run` in its systemd unit.
    #[test]
    fn a_systemd_word_round_trips(s in tricky()) {
        prop_assert_eq!(systemd_decode(&isoloom_render::configure::systemd_word(&s)), Some(s));
    }

    /// A template's filled-in text, as a VM's set-up carries it: base64 in a shell word.
    #[test]
    fn a_templates_text_round_trips_through_its_encoding(s in tricky()) {
        let word = quote(&isoloom_spec::shell::base64(s.as_bytes()));
        prop_assert_eq!(sh_decode(&word).as_deref().and_then(b64_decode), Some(s.into_bytes()));
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
