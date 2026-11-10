//! The spec's `tags` in the forms the places that carry them take: as written (AWS, Azure,
//! Oracle Cloud), as labels (Google Cloud, Kubernetes: lowercase letters, digits, `_` and `-`,
//! at most 63), or as `key:value` words (DigitalOcean, Linode); and the keys that would clash in
//! one of those forms, which validation refuses.

use indexmap::IndexMap;

use crate::model::Spec;

/// The longest label key or value Google Cloud and Kubernetes take.
const LABEL_MAX: usize = 63;

/// The first key of `tags` that clashes with an earlier one where a cloud carries them, with
/// why: the two equal but for case (Azure's tag names ignore case), or one label (Google Cloud's
/// and Kubernetes' [`labels`]).
pub(crate) fn clash(tags: &IndexMap<String, String>) -> Option<(&str, String)> {
    let keys: Vec<&str> = tags.keys().map(String::as_str).collect();
    keys.iter().enumerate().find_map(|(i, key)| {
        keys.iter().take(i).find_map(|earlier| {
            if earlier.eq_ignore_ascii_case(key) {
                Some((*key, format!("`{earlier}` and `{key}` are one tag on Azure, whose tag names ignore case")))
            } else {
                let same = label(earlier).filter(|l| Some(l) == label(key).as_ref())?;
                Some((*key, format!("`{earlier}` and `{key}` are one label, `{same}`, on Google Cloud and Kubernetes")))
            }
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
        let (label_key, label_why) = clash(&labels).unwrap();
        assert_eq!(
            (label_key, label_why.as_str()),
            (
                "cost_center",
                "`Cost.Center` and `cost_center` are one label, `cost_center`, on Google Cloud and Kubernetes"
            )
        );
        let cases = tags(&["Team", "team"]);
        let (case_key, case_why) = clash(&cases).unwrap();
        assert_eq!(
            (case_key, case_why.as_str()),
            ("team", "`Team` and `team` are one tag on Azure, whose tag names ignore case")
        );
        assert_eq!(clash(&tags(&["team", "cost-center", "cost_centre"])), None);
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
