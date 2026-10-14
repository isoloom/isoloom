//! Drafting a spec from files people already have. The draft is a starting point: every
//! import also returns notes for what the source said that the draft doesn't.

use std::net::Ipv4Addr;

use indexmap::IndexMap;

use crate::validate::Cidr;

pub mod compose;
pub mod terraform;
pub mod vagrant;
mod yaml;

/// A drafted `isoloom.yml` and the notes on it.
#[derive(Debug, Clone)]
pub struct Draft {
    pub yaml: String,
    pub notes: Vec<Note>,
}

/// Something the source said, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Where in the source, e.g. `services.web.ports`.
    pub at: String,
    pub kind: NoteKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoteKind {
    /// Changed on the way in (renamed, re-addressed): check it.
    Changed,
    /// Belongs in the machine's image or provisioning.
    InImage,
    /// Not expressible in the format yet.
    NotYet,
    /// The same effect comes another way.
    Equivalent,
    /// Deliberately not expressible.
    ByDesign,
    /// Written by Isoloom itself when it generates.
    Written,
    /// The source format's tooling; nothing to carry over.
    Tooling,
}

impl NoteKind {
    pub const ALL: [NoteKind; 7] = [
        NoteKind::Changed,
        NoteKind::InImage,
        NoteKind::NotYet,
        NoteKind::Equivalent,
        NoteKind::ByDesign,
        NoteKind::Written,
        NoteKind::Tooling,
    ];

    pub fn title(self) -> &'static str {
        match self {
            NoteKind::Changed => "Changed on the way in: check these",
            NoteKind::InImage => "Belongs in the image",
            NoteKind::NotYet => "Not expressible yet",
            NoteKind::Equivalent => "Same effect another way",
            NoteKind::ByDesign => "Left out: not every target can do it",
            NoteKind::Written => "Written by Isoloom when it generates",
            NoteKind::Tooling => "Tooling, nothing to carry over",
        }
    }
}

/// The notes an import collects as it reads, in the order it reads.
#[derive(Default)]
struct Notes(Vec<Note>);

impl Notes {
    fn add(&mut self, at: impl Into<String>, kind: NoteKind, text: impl Into<String>) {
        self.0.push(Note {
            at: at.into(),
            kind,
            text: text.into(),
        });
    }

    /// The finished draft: its YAML, and these notes on it.
    fn draft(self, yaml: String) -> Draft {
        Draft { yaml, notes: self.0 }
    }
}

/// Turns a source's name into a DNS label (`my_app` -> `my-app`).
pub fn label(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    match out.chars().next() {
        None => "env".into(),
        Some(c) if c.is_ascii_digit() => format!("m-{out}"),
        _ => out.chars().take(63).collect(),
    }
}

/// A network block as the draft writes it, e.g. `10.0.1.0/24`.
fn cidr_text(c: Cidr) -> String {
    format!("{}/{}", Ipv4Addr::from(c.base), c.len)
}

/// Names for the draft's networks, keyed like the networks: the name the source gave, else
/// `lab`, `lab-2`... by position. A name already taken gets `-2` until it's free.
fn network_names(networks: impl IntoIterator<Item = (String, Option<String>)>) -> IndexMap<String, String> {
    let mut names: IndexMap<String, String> = IndexMap::new();
    for (i, (key, named)) in networks.into_iter().enumerate() {
        let mut n = named.unwrap_or_else(|| if i == 0 { "lab".to_string() } else { format!("lab-{}", i + 1) });
        while names.values().any(|x| x == &n) {
            n.push_str("-2");
        }
        names.insert(key, n);
    }
    names
}
