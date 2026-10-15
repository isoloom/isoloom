//! The names a draft gives: a source's names made into DNS labels the spec accepts, each one
//! unique where it must be.

/// The longest name a draft gives: the strictest of the spec's limits, a machine's 53 characters
/// (its Kubernetes Service is `<name>-published`).
const MAX: usize = 53;

/// A source's name as a DNS label (`My_App 2` -> `my-app-2`): lowercase letters, digits and
/// dashes, starting with a letter (`9lives` -> `m-9lives`), at most 53 characters, never ending
/// on a dash; `env` when nothing is left.
pub(crate) fn label(source: &str) -> String {
    let mut words = String::new();
    for c in source.chars() {
        if c.is_ascii_alphanumeric() {
            words.push(c.to_ascii_lowercase());
        } else if !words.is_empty() && !words.ends_with('-') {
            words.push('-');
        }
    }
    let word = words.trim_end_matches('-');
    match word.chars().next() {
        None => "env".to_owned(),
        Some(c) if c.is_ascii_digit() => fit(&format!("m-{word}"), MAX),
        Some(_) => fit(word, MAX),
    }
}

/// `name` cut to at most `max` characters, never ending on a dash.
fn fit(name: &str, max: usize) -> String {
    name.chars().take(max).collect::<String>().trim_end_matches('-').to_owned()
}

/// `base`, else `base-2`, `base-3`...: the first that `taken` doesn't hold, at most 53 characters.
pub(crate) fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    // `taken` holds finitely many names, so one of these is free.
    (2..=u32::MAX)
        .map(|n| {
            let suffix = format!("-{n}");
            format!("{}{suffix}", fit(base, MAX.saturating_sub(suffix.len())))
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// Names for the draft's networks, keyed like the networks: the name the source gave, else
/// `lab`, `lab-2`... by position, each one unique.
pub(crate) fn network_names(networks: impl IntoIterator<Item = (String, Option<String>)>) -> indexmap::IndexMap<String, String> {
    let mut names = indexmap::IndexMap::new();
    for (i, (key, named)) in networks.into_iter().enumerate() {
        let base = named.unwrap_or_else(|| if i == 0 { "lab".to_owned() } else { format!("lab-{}", i + 1) });
        let name = unique(&base, |n| names.values().any(|taken: &String| taken == n));
        names.insert(key, name);
    }
    names
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_source_name_becomes_a_dns_label() {
        assert_eq!(label("My_App 2"), "my-app-2");
        assert_eq!(label("9lives"), "m-9lives");
        assert_eq!(label("__"), "env");
        assert_eq!(label("db.main"), "db-main");
    }

    #[test]
    fn a_name_taken_gets_the_next_free_number() {
        let taken = ["web", "web-2"];
        assert_eq!(unique("web", |n| taken.contains(&n)), "web-3");
        assert_eq!(unique("db", |n| taken.contains(&n)), "db");
    }

    #[test]
    fn unnamed_networks_are_lab_by_position_and_a_name_given_twice_is_numbered() {
        let names = network_names([("a".to_owned(), None), ("b".to_owned(), Some("lab".to_owned())), ("c".to_owned(), None)]);
        assert_eq!(names.values().map(String::as_str).collect::<Vec<_>>(), ["lab", "lab-2", "lab-3"]);
    }

    /// Whether `s` is a name the spec accepts for a machine.
    fn machine_name(s: &str) -> bool {
        s.len() <= MAX
            && s.starts_with(|c: char| c.is_ascii_lowercase())
            && !s.ends_with('-')
            && !s.contains("--")
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    proptest! {
        #[test]
        fn every_label_is_a_machine_name(source in ".{0,80}") {
            let l = label(&source);
            prop_assert!(machine_name(&l), "{source:?} -> {l:?}");
        }

        #[test]
        fn a_unique_name_is_free_and_still_a_machine_name(source in "[a-z][a-z0-9-]{0,70}", taken in proptest::collection::vec(0u32..6, 0..6)) {
            let base = label(&source);
            let taken: Vec<String> = std::iter::once(base.clone()).chain(taken.iter().map(|n| format!("{}-{n}", fit(&base, MAX - 2)))).collect();
            let name = unique(&base, |n| taken.iter().any(|t| t == n));
            prop_assert!(!taken.contains(&name), "{name} is taken");
            prop_assert!(machine_name(&name), "{name:?}");
        }
    }
}
