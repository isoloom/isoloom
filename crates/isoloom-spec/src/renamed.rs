//! Machine keys renamed before 1.0, still read with a warning for at least one release (see
//! docs/stability.md): a machine's `services` (its ports, protocols, probes, published and
//! dormant ones) is now `ports`, which leaves the word "service" to what the machine runs. The
//! old key moves to the new one on the YAML, before anything else reads it, and each move
//! becomes a warning `isoloom validate` prints, so the author renames it before the old name
//! goes away.

use serde_yaml_ng::Value;

use crate::validate::Problem;

/// A machine's old keys, each with the key it became.
pub const MACHINE_KEYS: &[(&str, &str)] = &[("services", "ports")];

/// Moves each machine's old keys to their new names, in place, and returns one warning per
/// move. A machine that sets both the old and the new key is refused: which one is meant is
/// ambiguous.
pub fn machine_keys(doc: &mut Value) -> Result<Vec<Problem>, String> {
    let mut warnings = Vec::new();
    let Some(Value::Mapping(machines)) = doc.get_mut("machines") else {
        return Ok(warnings);
    };
    for (name, m) in machines.iter_mut() {
        let (Some(name), Value::Mapping(m)) = (name.as_str(), m) else { continue };
        for (old, new) in MACHINE_KEYS {
            let Some(v) = m.shift_remove(*old) else { continue };
            if m.contains_key(*new) {
                return Err(format!(
                    "machines.{name}: sets both `{new}` and `{old}`, its name before 1.0; keep `{new}` only"
                ));
            }
            m.insert(Value::String((*new).to_string()), v);
            warnings.push(Problem::new(
                format!("machines.{name}.{old}"),
                format!("`{old}` is now `{new}`: rename the key (`{old}` is still read, with this warning, until a later release removes it)"),
            ));
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(yaml: &str) -> (Result<Vec<Problem>, String>, Value) {
        let mut doc: Value = serde_yaml_ng::from_str(yaml).unwrap();
        (machine_keys(&mut doc), doc)
    }

    #[test]
    fn the_old_key_moves_with_a_warning() {
        let (warnings, doc) = moved("machines:\n  web: { services: [{ port: 80 }] }\n  db: { ports: [{ port: 5432 }] }\n");
        assert_eq!(
            warnings.unwrap(),
            [Problem::new(
                "machines.web.services",
                "`services` is now `ports`: rename the key (`services` is still read, with this warning, until a later release removes it)"
            )]
        );
        assert_eq!(doc["machines"]["web"]["ports"][0]["port"], Value::from(80));
        assert!(doc["machines"]["web"].get("services").is_none());
    }

    #[test]
    fn both_keys_are_refused() {
        let (out, _) = moved("machines:\n  web: { services: [{ port: 80 }], ports: [{ port: 81 }] }\n");
        assert_eq!(
            out.unwrap_err(),
            "machines.web: sets both `ports` and `services`, its name before 1.0; keep `ports` only"
        );
    }

    #[test]
    fn a_spec_without_machines_is_left_alone() {
        assert_eq!(moved("version: 1\n").0, Ok(vec![]));
    }
}
