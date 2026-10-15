//! Drafting a spec from files people already have. The draft is a starting point: every
//! import also returns notes for what the source said that the draft doesn't.

use std::net::Ipv4Addr;

use indexmap::IndexMap;

use crate::validate::Cidr;
use isoloom_spec::{images, model, schema, validate};

pub mod compose;
mod error;
pub mod terraform;
pub mod vagrant;
mod yaml;

pub use error::ImportError;

/// A whole, non-negative number of `x` (already rounded by the caller) as a `u32`; `None` when it
/// doesn't fit (negative, too large, not a number).
pub(crate) fn whole_u32(x: f64) -> Option<u32> {
    // Checked against u32's range first, so the cast neither truncates nor loses a sign.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (x.is_finite() && (0.0..=f64::from(u32::MAX)).contains(&x)).then_some(x as u32)
}

/// A drafted `isoloom.yml` and the notes on it.
#[derive(Debug, Clone)]
pub struct Draft {
    /// The drafted spec, as YAML ready to write to `isoloom.yml`.
    pub yaml: String,
    /// What the source said that the draft doesn't carry as it was, grouped by [`NoteKind`].
    pub notes: Vec<Note>,
}

/// Something the source said, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Where in the source, e.g. `services.web.ports`.
    pub at: String,
    /// What became of it, which says whether it needs the user's attention.
    pub kind: NoteKind,
    /// The note itself: what was dropped or changed, and what to do about it.
    pub text: String,
}

/// What became of something the source said, from what needs checking to what needs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoteKind {
    /// Changed on the way in (renamed, re-addressed) or left out: check it.
    Changed,
    /// Belongs in the machine's image or provisioning.
    InImage,
    /// Not expressible in the format yet.
    NotYet,
    /// The same effect comes another way.
    Equivalent,
    /// Deliberately not expressible.
    ByDesign,
    /// The source format's tooling; nothing to carry over.
    Tooling,
}

impl NoteKind {
    /// Every kind, in the order the notes are listed.
    pub const ALL: [NoteKind; 6] = [
        NoteKind::Changed,
        NoteKind::InImage,
        NoteKind::NotYet,
        NoteKind::Equivalent,
        NoteKind::ByDesign,
        NoteKind::Tooling,
    ];

    /// The heading the notes of this kind are listed under.
    pub fn title(self) -> &'static str {
        match self {
            NoteKind::Changed => "Changed on the way in: check these",
            NoteKind::InImage => "Belongs in the image",
            NoteKind::NotYet => "Not expressible yet",
            NoteKind::Equivalent => "Same effect another way",
            NoteKind::ByDesign => "Left out: not every target can do it",
            NoteKind::Tooling => "Tooling, nothing to carry over",
        }
    }
}

/// The note on something the source says that the import doesn't read: it is left out, never
/// dropped silently.
pub(crate) const LEFT_OUT: &str = "not read by the import: left out; carry it over by hand if the environment needs it";

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
