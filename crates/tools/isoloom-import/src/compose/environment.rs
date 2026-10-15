//! A service's `environment`: its entries, and the shell variables a value reads, which become
//! the draft's inputs.

use serde_yaml_ng::Value;

use super::text::str_of;

/// Variables a Compose value reads from the shell: `${NAME}`, `${NAME:-x}`, `$NAME`; `$$` is a
/// literal dollar.
pub(super) fn variables(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once('$') {
        if let Some(escaped) = after.strip_prefix('$') {
            rest = escaped;
            continue;
        }
        let (braced, from) = after.strip_prefix('{').map_or((false, after), |inner| (true, inner));
        let end = from.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(from.len());
        let Some((name, tail)) = from.split_at_checked(end) else { break };
        // `${NAME` without its `}` reads nothing.
        if !name.is_empty() && (!braced || !tail.is_empty()) {
            found.push(name.to_owned());
        }
        rest = tail;
    }
    found
}

/// An input name from an environment variable name, as written (the shell reads it by that name).
pub(super) fn input_name(s: &str) -> Option<String> {
    let ok = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit());
    ok.then(|| s.to_owned())
}

/// The `environment` entries, mapping or list, as (name, value); `- NAME` alone has no value.
pub(super) fn env_entries(v: &Value) -> Vec<(String, Option<String>)> {
    match v {
        Value::Mapping(env) => env.iter().map(|(k, value)| (str_of(k).unwrap_or_default(), str_of(value))).collect(),
        Value::Sequence(list) => list
            .iter()
            .filter_map(str_of)
            .map(|e| match e.split_once('=') {
                Some((k, value)) => (k.to_owned(), Some(value.to_owned())),
                None => (e, None),
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn every_form_of_a_variable_is_read_and_a_double_dollar_is_not_one() {
        assert_eq!(variables("${A} $B ${C:-x} $$D ${E"), ["A", "B", "C"]);
        assert_eq!(variables("cost: 5$"), Vec::<String>::new());
    }

    #[test]
    fn an_environment_list_entry_without_a_value_has_none() -> Result<(), serde_yaml_ng::Error> {
        let list: Value = serde_yaml_ng::from_str("[A=1, B]")?;
        assert_eq!(env_entries(&list), [("A".to_owned(), Some("1".to_owned())), ("B".to_owned(), None)]);
        Ok(())
    }

    proptest! {
        #[test]
        fn a_variable_written_any_way_is_found(name in "[A-Za-z_][A-Za-z0-9_]{0,20}", before in "[^$]{0,10}", after in "[^A-Za-z0-9_$]{1,10}") {
            for written in [format!("{before}${{{name}}}{after}"), format!("{before}${name}{after}"), format!("{before}${{{name}:-x}}")] {
                prop_assert_eq!(variables(&written), vec![name.clone()], "in {}", written);
            }
        }

        #[test]
        fn every_variable_found_is_a_name_after_a_dollar(text in ".{0,40}") {
            for name in variables(&text) {
                prop_assert!(!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
                let (bare, braced) = (format!("${name}"), format!("${{{name}"));
                prop_assert!(text.contains(&bare) || text.contains(&braced), "{} isn't after a dollar in {}", name, text);
            }
        }

        #[test]
        fn a_value_with_no_dollar_reads_no_variable(text in "[^$]{0,40}") {
            prop_assert!(variables(&text).is_empty());
        }
    }
}
