//! Where each field is written: a field path → line and column index, read from the YAML's
//! events once at load, so every problem (lowering's included) points at its line.

use std::collections::BTreeMap;

use saphyr_parser::{Event, Parser};

use super::problem::{FieldPath, Problem, Span};

/// Each written field's place in the file, by its path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spans(BTreeMap<FieldPath, Span>);

/// A mapping or a list being read, and where its next item goes.
enum Frame {
    /// A mapping: the key of the value being read, if a key was read.
    Map { path: FieldPath, key: Option<String> },
    /// A list: the index of its next item.
    Seq { path: FieldPath, next: usize },
}

impl Spans {
    /// The places of every key and list item in `yaml`. A document the parser refuses gives the
    /// places read before the refusal (the YAML error itself carries its own).
    #[must_use]
    pub fn of(yaml: &str) -> Spans {
        let mut spans = Spans::default();
        let mut stack: Vec<Frame> = Vec::new();
        for (event, span) in Parser::new_from_str(yaml).map_while(Result::ok) {
            let at = Span {
                line: span.start.line(),
                column: span.start.col() + 1,
            };
            match event {
                Event::Scalar(text, ..) => spans.scalar(&mut stack, &text, at),
                Event::Alias(_) => {
                    spans.value(&mut stack, at);
                }
                Event::MappingStart(..) | Event::SequenceStart(..) => {
                    let path = spans.value(&mut stack, at).unwrap_or_default();
                    stack.push(if matches!(event, Event::MappingStart(..)) {
                        Frame::Map { path, key: None }
                    } else {
                        Frame::Seq { path, next: 0 }
                    });
                }
                Event::MappingEnd | Event::SequenceEnd => {
                    stack.pop();
                }
                _ => {}
            }
        }
        spans
    }

    /// A scalar: a mapping's key (its place recorded), or a value.
    fn scalar(&mut self, stack: &mut [Frame], text: &str, at: Span) {
        if let Some(Frame::Map { path, key: key @ None }) = stack.last_mut() {
            self.0.entry(path.key(text)).or_insert(at);
            *key = Some(text.to_string());
        } else {
            self.value(stack, at);
        }
    }

    /// A value starts at `at`: its path, once the frame it is in has moved on to the next item.
    fn value(&mut self, stack: &mut [Frame], at: Span) -> Option<FieldPath> {
        match stack.last_mut()? {
            Frame::Map { path, key } => Some(path.key(&key.take()?)),
            Frame::Seq { path, next } => {
                let item = path.index(*next);
                *next += 1;
                self.0.entry(item.clone()).or_insert(at);
                Some(item)
            }
        }
    }

    /// The place of `path`: its own, else its nearest written parent's (a field a group gives a
    /// machine is placed at the machine).
    #[must_use]
    pub fn find(&self, path: &FieldPath) -> Option<Span> {
        let mut at = Some(path.clone());
        while let Some(p) = at {
            if let Some(s) = self.0.get(&p) {
                return Some(*s);
            }
            at = p.parent();
        }
        None
    }

    /// `problem`, placed (unless it already is).
    #[must_use]
    pub fn place(&self, mut problem: Problem) -> Problem {
        if problem.span.is_none() {
            problem.span = self.find(&problem.path);
        }
        problem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_items_are_placed_in_block_and_flow_style() {
        let yaml = "version: 1\nmachines:\n  web:\n    networks: { lab: 10 }\n    files:\n      - { path: /a, input: FLAG }\n      - path: /b\n";
        let spans = Spans::of(yaml);
        let at = |p: &str| spans.find(&FieldPath::new(p)).map(|s| (s.line, s.column));
        assert_eq!(at("version"), Some((1, 1)));
        assert_eq!(at("machines.web"), Some((3, 3)));
        assert_eq!(at("machines.web.networks.lab"), Some((4, 17)));
        assert_eq!(at("machines.web.files[0]"), Some((6, 9)));
        assert_eq!(at("machines.web.files[0].input"), Some((6, 21)));
        assert_eq!(at("machines.web.files[1].path"), Some((7, 9)));
        // A field that isn't written takes its nearest written parent's place.
        assert_eq!(at("machines.web.resources.cpus"), Some((3, 3)));
        assert_eq!(at("checks"), None);
    }
}
