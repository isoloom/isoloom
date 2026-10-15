//! Compose interpolates `$NAME` and `${NAME}` in a file's values from its own environment.
//! Isoloom's values are text of its own (scripts, rules, a machine's settings), so they are
//! written literal; only the references Isoloom means stay for Compose to fill (where a port is
//! published, a machine's inputs).

/// `text` as Compose reads it back unchanged: every `$` doubled.
pub(crate) fn literal(text: &str) -> String {
    text.replace('$', "$$")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::literal;

    #[test]
    fn a_reference_is_kept_from_compose() {
        assert_eq!(literal("echo ${HOME} $PATH"), "echo $${HOME} $$PATH");
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
