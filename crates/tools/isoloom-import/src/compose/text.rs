//! A Compose value as text: a scalar as itself, anything as one line for a note.

use serde_yaml_ng::Value;

/// A scalar as text: a string, a number or a boolean.
pub(super) fn str_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A Compose value on one line, for notes.
pub(super) fn flat(v: &Value) -> String {
    match v {
        Value::Sequence(items) => items.iter().map(flat).collect::<Vec<_>>().join(" "),
        Value::Mapping(m) => m.iter().map(|(k, item)| format!("{}={}", flat(k), flat(item))).collect::<Vec<_>>().join(" "),
        other => str_of(other).unwrap_or_default(),
    }
}

/// The parts of a short entry (`ports`, `volumes`), split on `:` outside `${...}`: a variable's
/// default (`${DATA:-./data}`) is one part with its variable.
pub(super) fn colon_parts(entry: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0_u32, String::new());
    for ch in entry.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    parts.push(cur);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scalar_is_text_and_a_collection_one_line() -> Result<(), serde_yaml_ng::Error> {
        assert_eq!(str_of(&serde_yaml_ng::from_str("8080")?), Some("8080".to_owned()));
        assert_eq!(str_of(&serde_yaml_ng::from_str("[a]")?), None);
        assert_eq!(flat(&serde_yaml_ng::from_str("{ a: [1, 2], b: x }")?), "a=1 2 b=x");
        Ok(())
    }
}
