//! Reading YAML the way Compose does: a file that sets a key twice is still a Compose file.

use std::cell::RefCell;

use serde_yaml_ng::{Mapping, Value};

use super::text::{flat, str_of};

/// A key set twice in one mapping: where, which key, and the value kept (the last).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Duplicate {
    pub(super) at: String,
    pub(super) key: String,
    pub(super) kept: String,
}

/// Reads YAML the way Compose does: a key set twice in one mapping keeps its last value
/// (`serde_yaml_ng` refuses the file instead). Every such key is recorded with where it was.
pub(super) fn lenient_yaml(text: &str) -> Result<(Value, Vec<Duplicate>), serde_yaml_ng::Error> {
    use serde::de::DeserializeSeed;
    let dups = RefCell::new(Vec::new());
    let value = Lenient {
        at: String::new(),
        dups: &dups,
    }
    .deserialize(serde_yaml_ng::Deserializer::from_str(text))?;
    Ok((value, dups.into_inner()))
}

/// A YAML value at a path (`services.api.environment`), tolerating duplicate keys below it.
struct Lenient<'a> {
    at: String,
    dups: &'a RefCell<Vec<Duplicate>>,
}

impl Lenient<'_> {
    /// Records `name` as set twice at this mapping, keeping the value of the last one.
    fn duplicate(&self, name: String, value: &Value) {
        let mut dups = self.dups.borrow_mut();
        let kept = flat(value);
        match dups.iter_mut().find(|d| d.at == self.at && d.key == name) {
            Some(d) => d.kept = kept,
            None => dups.push(Duplicate {
                at: self.at.clone(),
                key: name,
                kept,
            }),
        }
    }
}

impl<'de> serde::de::DeserializeSeed<'de> for Lenient<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for Lenient<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any YAML value")
    }
    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }
    fn visit_i64<E>(self, i: i64) -> Result<Value, E> {
        Ok(Value::Number(i.into()))
    }
    fn visit_u64<E>(self, u: u64) -> Result<Value, E> {
        Ok(Value::Number(u.into()))
    }
    fn visit_f64<E>(self, f: f64) -> Result<Value, E> {
        Ok(Value::Number(f.into()))
    }
    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::String(s.to_owned()))
    }
    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::String(s))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(Lenient {
            at: format!("{}.{}", self.at, items.len()),
            dups: self.dups,
        })? {
            items.push(item);
        }
        Ok(Value::Sequence(items))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut mapping = Mapping::new();
        while let Some(key) = map.next_key::<Value>()? {
            let name = str_of(&key).unwrap_or_else(|| flat(&key));
            let at = if self.at.is_empty() { name.clone() } else { format!("{}.{name}", self.at) };
            let value = map.next_value_seed(Lenient { at, dups: self.dups })?;
            if mapping.contains_key(&key) {
                // Compose keeps the last one; so does `insert`, in the first one's place.
                self.duplicate(name, &value);
            }
            mapping.insert(key, value);
        }
        Ok(Value::Mapping(mapping))
    }
    fn visit_enum<A: serde::de::EnumAccess<'de>>(self, data: A) -> Result<Value, A::Error> {
        use serde::de::{Error, VariantAccess};
        let (tag, contents) = data.variant::<String>()?;
        if tag.is_empty() {
            return Err(A::Error::custom("empty YAML tag is not allowed"));
        }
        let value = contents.newtype_variant_seed(self)?;
        Ok(Value::Tagged(Box::new(serde_yaml_ng::value::TaggedValue {
            tag: serde_yaml_ng::value::Tag::new(tag),
            value,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_set_twice_keeps_its_last_value_and_is_recorded_once() -> Result<(), serde_yaml_ng::Error> {
        let (value, dups) = lenient_yaml("a: { x: 1, x: 2, x: 3 }\nb: [1]\n")?;
        assert_eq!(value.get("a").and_then(|a| a.get("x")).and_then(Value::as_u64), Some(3));
        assert_eq!(
            dups,
            [Duplicate {
                at: "a".to_owned(),
                key: "x".to_owned(),
                kept: "3".to_owned()
            }]
        );
        Ok(())
    }

    #[test]
    fn text_that_isnt_yaml_is_refused() {
        assert!(lenient_yaml("a: [").is_err());
    }
}
