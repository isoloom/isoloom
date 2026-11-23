//! Compose interpolates `$NAME` and `${NAME}` in a file's values from its own environment.
//! Isoloom's values are text of its own (scripts, rules, a machine's settings), so they are
//! written literal; only the references Isoloom means stay for Compose to fill (where a port is
//! published, a machine's inputs).

/// `text` as Compose reads it back unchanged: every `$` doubled.
pub(crate) fn literal(text: &str) -> String {
    text.replace('$', "$$")
}

/// Input `name`'s value as Compose fills it from its environment: `${NAME:-default}`, the
/// default (literal) when the variable is unset or empty, else empty. Only an environment value's
/// default is written here: a spec refuses a `}` in it, which would end the reference.
pub(crate) fn input(name: &str, default: Option<&str>) -> String {
    format!("${{{name}:-{}}}", literal(default.unwrap_or_default()))
}

/// `mark` when input `name` is given (its variable set and not empty), else nothing:
/// `${NAME:+mark}`. `mark` holds no `}` nor `$`.
pub(crate) fn given(name: &str, mark: &str) -> String {
    format!("${{{name}:+{mark}}}")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{input, literal};

    #[test]
    fn a_reference_is_kept_from_compose() {
        assert_eq!(literal("echo ${HOME} $PATH"), "echo $${HOME} $$PATH");
    }

    #[test]
    fn an_input_falls_back_to_its_default_written_literal() {
        assert_eq!(input("MODE", None), "${MODE:-}");
        assert_eq!(input("TOKEN", Some("dev-$HOME")), "${TOKEN:-dev-$$HOME}");
    }

    proptest! {
        #[test]
        fn compose_reads_back_the_text_and_finds_no_reference(text in ".*") {
            let written = literal(&text);
            // Every run of `$` is even: Compose reads each pair as one `$`, never a reference.
            let mut runs = written.split(|c| c != '$').filter(|run| !run.is_empty());
            prop_assert!(runs.all(|run| run.len() % 2 == 0), "{written}");
            prop_assert_eq!(written.replace("$$", "$"), text);
        }
    }
}
