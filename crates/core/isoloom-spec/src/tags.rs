//! The spec's `tags` in the forms the places that carry them take: as written (AWS, Azure,
//! Oracle Cloud), as labels (Google Cloud, Kubernetes: lowercase letters, digits, `_` and `-`,
//! at most 63), or as `key:value` words (DigitalOcean, Linode); the rules a tag follows so every
//! one of those places takes it; and the keys that would clash in one of those forms. Validation
//! refuses a spec's tag that breaks them, and the defaults one of theirs.

use indexmap::IndexMap;

use crate::model::Spec;

/// The longest label key or value Google Cloud and Kubernetes take: a DNS label's length.
const LABEL_MAX: usize = crate::validate::DNS_LABEL_MAX;

/// The longest and shortest tag Linode takes, as its `key:value` word: the strictest cloud's.
const WORD: std::ops::RangeInclusive<usize> = 3..=50;

/// Why `key` can't be a tag's key everywhere, if it can't: at most 63 letters, digits and
/// `_ . : -`, starting with a letter (Google Cloud's labels), and not one Isoloom writes itself in
/// any case (Azure's tag names ignore case).
#[must_use]
pub(crate) fn key_refusal(key: &str) -> Option<String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "_.:-".contains(c);
    let lower = key.to_ascii_lowercase();
    if key.len() > LABEL_MAX || !key.starts_with(|c: char| c.is_ascii_alphabetic()) || !key.chars().all(allowed) {
        Some("a key of at most 63 letters, digits and `_ . : -`, starting with a letter, e.g. cost-center".to_owned())
    } else if lower.starts_with("isoloom") || lower == "managed-by" {
        Some(format!("`{key}` is Isoloom's own tag: name yours otherwise"))
    } else {
        None
    }
}

/// Why `value` can't be tag `key`'s value everywhere, if it can't: on one line, and with the key
/// a `key:value` word of 3 to 50 characters (Linode's tags).
#[must_use]
pub(crate) fn value_refusal(key: &str, value: &str) -> Option<String> {
    let word = key.chars().count() + 1 + value.chars().count();
    if value.chars().any(char::is_control) {
        Some("a value on one line".to_owned())
    } else if !WORD.contains(&word) {
        let most = WORD.end().saturating_sub(key.chars().count() + 1);
        Some(format!(
            "`{key}:<value>` is at most {} characters and at least {} (Linode's tags): a value of 1 to {most} characters here",
            WORD.end(),
            WORD.start()
        ))
    } else {
        None
    }
}

/// Two keys of one set of tags that clash where a cloud carries them, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Clash<'a> {
    /// The first of the two.
    pub(crate) earlier: &'a str,
    /// The one that clashes with it.
    pub(crate) key: &'a str,
    /// Why they are one.
    pub(crate) why: String,
}

/// The first key of `tags` that clashes with an earlier one where a cloud carries them: the two
/// equal but for case (Azure's tag names ignore case), or one label (Google Cloud's and
/// Kubernetes' [`labels`]).
pub(crate) fn clash(tags: &IndexMap<String, String>) -> Option<Clash<'_>> {
    let keys: Vec<&str> = tags.keys().map(String::as_str).collect();
    keys.iter().enumerate().find_map(|(i, key)| {
        keys.iter().take(i).find_map(|earlier| {
            let why = if earlier.eq_ignore_ascii_case(key) {
                format!("`{earlier}` and `{key}` are one tag on Azure, whose tag names ignore case")
            } else {
                let same = label(earlier).filter(|l| Some(l) == label(key).as_ref())?;
                format!("`{earlier}` and `{key}` are one label, `{same}`, on Google Cloud and Kubernetes")
            };
            Some(Clash { earlier, key, why })
        })
    })
}

/// The tags as written.
#[must_use]
pub fn written(spec: &Spec) -> Vec<(String, String)> {
    spec.tags.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// The tags as labels: each key and value lowercased, any other character `_`, at most 63
/// characters, starting and ending with a letter or digit (a key that would be empty is left out).
#[must_use]
pub fn labels(spec: &Spec) -> Vec<(String, String)> {
    spec.tags
        .iter()
        .filter_map(|(k, v)| Some((label(k).filter(|k| !k.is_empty())?, label(v).unwrap_or_default())))
        .collect()
}

/// The tags as `key:value` words of letters, digits, `:`, `_` and `-`.
#[must_use]
pub fn words(spec: &Spec) -> Vec<String> {
    let word = |s: &str| {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect::<String>()
    };
    spec.tags.iter().map(|(k, v)| format!("{}:{}", word(k), word(v))).collect()
}

/// `s` as a label: `None` when nothing of it is left.
fn label(s: &str) -> Option<String> {
    let mapped: String = s
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(LABEL_MAX)
        .collect();
    let trimmed = mapped.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(tags: &[(&str, &str)]) -> Spec {
        let mut spec =
            crate::testing::model(&crate::testing::lab("machines:\n  web: { os: debian@12, networks: { lan: 10 } }\n")).expect("the test spec reads");
        spec.tags = tags.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        spec
    }

    #[test]
    fn each_place_gets_the_form_it_takes() {
        let s = spec(&[("Cost.Center", "R&D 42"), ("team", "platform")]);
        assert_eq!(
            written(&s),
            [("Cost.Center".to_owned(), "R&D 42".to_owned()), ("team".to_owned(), "platform".to_owned())]
        );
        assert_eq!(
            labels(&s),
            [("cost_center".to_owned(), "r_d_42".to_owned()), ("team".to_owned(), "platform".to_owned())]
        );
        assert_eq!(words(&s), ["Cost_Center:R_D_42", "team:platform"]);
    }

    #[test]
    fn keys_equal_but_for_case_or_one_label_clash() {
        let tags = |keys: &[&str]| keys.iter().map(|k| ((*k).to_owned(), String::new())).collect::<IndexMap<_, _>>();
        let labels = tags(&["team", "Cost.Center", "cost_center"]);
        let label = clash(&labels).unwrap();
        assert_eq!(
            (label.earlier, label.key, label.why.as_str()),
            (
                "Cost.Center",
                "cost_center",
                "`Cost.Center` and `cost_center` are one label, `cost_center`, on Google Cloud and Kubernetes"
            )
        );
        let cases = tags(&["Team", "team"]);
        let case = clash(&cases).unwrap();
        assert_eq!(
            (case.key, case.why.as_str()),
            ("team", "`Team` and `team` are one tag on Azure, whose tag names ignore case")
        );
        assert_eq!(clash(&tags(&["team", "cost-center", "cost_centre"])), None);
    }

    #[test]
    fn a_tag_is_one_every_cloud_takes() {
        assert_eq!(key_refusal("cost-center"), None);
        for key in ["9x", "a b", &"k".repeat(64)] {
            assert!(key_refusal(key).is_some_and(|w| w.starts_with("a key of at most 63")), "{key}");
        }
        assert!(key_refusal("Managed-By").is_some_and(|w| w.contains("Isoloom's own")));
        assert_eq!(value_refusal("team", "ops"), None);
        // Linode's `key:value` is 3 to 50 characters, counted as characters.
        assert_eq!(value_refusal("team", &"é".repeat(45)), None);
        let long = value_refusal("team", &"é".repeat(46)).unwrap_or_default();
        assert!(long.contains("a value of 1 to 45 characters here"), "{long}");
        assert!(value_refusal("x", "").is_some(), "`x:` is two characters");
        assert_eq!(value_refusal("x", "y"), None);
        assert_eq!(value_refusal("team", "a\nb").as_deref(), Some("a value on one line"));
    }

    #[test]
    fn a_label_is_at_most_63_characters_and_starts_and_ends_with_a_letter_or_digit() {
        let long = "x".repeat(70);
        let s = spec(&[("-a-", ""), (&long, "-v-"), ("..", "x")]);
        let l = labels(&s);
        assert_eq!(l[0], ("a".to_owned(), String::new()));
        assert_eq!((l[1].0.len(), l[1].1.as_str()), (63, "v"));
        assert_eq!(l.len(), 2, "a key with nothing left is left out");
    }
}
