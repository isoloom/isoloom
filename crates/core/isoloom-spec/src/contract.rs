//! The names and folders every format Isoloom writes shares with what reads them: the folder
//! its outputs are under, the prefix of the names it reserves, and the kebab-case rule of an
//! environment's name. A rename is a breaking change.

/// The folder Isoloom writes a project's outputs under (`.isoloom/<target>/`), and the start of
/// every instance's own (`.isoloom-<n>/`).
pub const OUTPUT_DIR: &str = ".isoloom";

/// What every name Isoloom reserves starts with: a machine, alias or network the spec names
/// can't start with it, and the names Isoloom adds (its controller, its router, its tags, a
/// Kubernetes Service) do.
pub const RESERVED_PREFIX: &str = "isoloom-";

/// The longest kebab-case name ([`is_kebab`]): an environment's name is a DNS label and a
/// Kubernetes namespace.
pub const KEBAB_MAX: usize = 63;

/// Kebab case, as an environment's name (`my-env`, `shop-2`): lowercase letters and digits in
/// words joined by single dashes, at most [`KEBAB_MAX`] characters. The one rule the spec's
/// kebab-case names follow.
#[must_use]
pub fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= KEBAB_MAX
        && s.split('-')
            .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_case_is_lowercase_words_joined_by_single_dashes() {
        assert!(is_kebab("shop-2") && is_kebab("a"));
        assert!(!is_kebab("") && !is_kebab("Shop") && !is_kebab("a--b") && !is_kebab("-a") && !is_kebab("a_b"));
        assert!(is_kebab(&"a".repeat(KEBAB_MAX)) && !is_kebab(&"a".repeat(KEBAB_MAX + 1)));
    }
}
