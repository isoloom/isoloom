//! Values that are text, kept as the author wrote them. YAML reads an unquoted `1.10` as the
//! number 1.1, `0x10` as 16 and `1e3` as 1000, so a value that is text by nature (an environment
//! variable, a launch value) would reach the machine rewritten. [`keep`] puts the written text
//! back for those, from the YAML's own events, before anything reads the document.

use saphyr_parser::{Event, Parser, ScalarStyle};
use serde_yaml_ng::Value;

/// One step into a document: a mapping's key, or a list's index.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(usize),
}

/// A mapping or a list being read.
enum Frame {
    /// A mapping: the key of the value being read, if a key was read.
    Map(Option<String>),
    /// A list: the index of its next item.
    Seq(usize),
}

/// Whether the value at `path` is an environment variable's (`<...>.environment.<NAME>`): the
/// machines', `common`'s, a group's, a profile's `spec`.
#[must_use]
pub(crate) fn environment(path: &[&str]) -> bool {
    matches!(path, [.., "environment", _])
}

/// Each number or boolean of `doc` written unquoted at a path `wanted` takes, as the text it is
/// written with in `yaml` (the document `doc` was read from).
pub(crate) fn keep(yaml: &str, doc: &mut Value, wanted: impl Fn(&[&str]) -> bool) {
    for (path, text) in plain_scalars(yaml) {
        let keys: Vec<&str> = path
            .iter()
            .map(|s| match s {
                Step::Key(k) => k.as_str(),
                Step::Index(_) => "",
            })
            .collect();
        if !wanted(&keys) {
            continue;
        }
        if let Some(node) = at(doc, &path)
            && matches!(node, Value::Number(_) | Value::Bool(_))
        {
            *node = Value::String(text);
        }
    }
}

/// Every plain (unquoted) scalar value of `yaml` with its path; a document the parser refuses
/// gives those read before the refusal.
fn plain_scalars(yaml: &str) -> Vec<(Vec<Step>, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<(Vec<Step>, Frame)> = Vec::new();
    let mut root: Vec<Step> = Vec::new();
    for (event, _) in Parser::new_from_str(yaml).map_while(Result::ok) {
        match event {
            Event::Scalar(text, style, ..) => {
                if let Some((_, Frame::Map(key @ None))) = stack.last_mut() {
                    *key = Some(text.to_string());
                    continue;
                }
                if let Some(path) = next(&mut stack, &mut root)
                    && style == ScalarStyle::Plain
                {
                    out.push((path, text.to_string()));
                }
            }
            Event::Alias(_) => {
                next(&mut stack, &mut root);
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => {
                let path = next(&mut stack, &mut root).unwrap_or_default();
                let frame = if matches!(event, Event::MappingStart(..)) {
                    Frame::Map(None)
                } else {
                    Frame::Seq(0)
                };
                stack.push((path, frame));
            }
            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
            }
            _ => {}
        }
    }
    out
}

/// The path of the value that starts now, the frame it is in moved on to its next item.
fn next(stack: &mut [(Vec<Step>, Frame)], root: &mut Vec<Step>) -> Option<Vec<Step>> {
    let Some((path, frame)) = stack.last_mut() else {
        return Some(std::mem::take(root));
    };
    let step = match frame {
        Frame::Map(key) => Step::Key(key.take()?),
        Frame::Seq(n) => {
            *n += 1;
            Step::Index(*n - 1)
        }
    };
    let mut out = path.clone();
    out.push(step);
    Some(out)
}

/// The node of `doc` at `path`, when there is one.
fn at<'a>(doc: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    path.iter().try_fold(doc, |node, step| match (node, step) {
        (Value::Mapping(m), Step::Key(k)) => m.get_mut(k.as_str()),
        (Value::Sequence(items), Step::Index(i)) => items.get_mut(*i),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(yaml: &str) -> Value {
        let mut doc: Value = serde_yaml_ng::from_str(yaml).unwrap();
        keep(yaml, &mut doc, environment);
        doc
    }

    #[test]
    fn an_environment_value_is_its_text_as_written() {
        let doc = kept("machines:\n  web:\n    environment: { V: 1.10, H: 0x10, E: 1e3, N: 007, B: true, Q: '1.10', S: text }\n    resources: { cpus: 2 }\n");
        let env = &doc["machines"]["web"]["environment"];
        for (k, v) in [
            ("V", "1.10"),
            ("H", "0x10"),
            ("E", "1e3"),
            ("N", "007"),
            ("B", "true"),
            ("Q", "1.10"),
            ("S", "text"),
        ] {
            assert_eq!(env[k].as_str(), Some(v), "{k}");
        }
        assert_eq!(
            doc["machines"]["web"]["resources"]["cpus"].as_u64(),
            Some(2),
            "only text fields are kept as text"
        );
    }

    #[test]
    fn block_style_lists_and_common_are_read_too() {
        let doc = kept("common:\n  environment:\n    V: 2.50\nchecks:\n  - x\n  - { environment: { W: 3.0 } }\n");
        assert_eq!(doc["common"]["environment"]["V"].as_str(), Some("2.50"));
        assert_eq!(doc["checks"][1]["environment"]["W"].as_str(), Some("3.0"));
    }
}
