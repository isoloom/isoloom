//! What an import gives back: the drafted `isoloom.yml`, and a note on everything the source said
//! that the draft doesn't carry as it was.

pub(crate) mod quoting;
pub(crate) mod writer;

use std::fmt;

/// A drafted `isoloom.yml` and the notes on it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Draft {
    /// The drafted spec, as YAML ready to write to `isoloom.yml`.
    pub yaml: String,
    /// What the source said that the draft doesn't carry as it was, grouped by [`NoteKind`].
    pub notes: Vec<Note>,
}

/// Something the source said, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Note {
    /// Where in the source, e.g. `services.web.ports`.
    pub at: String,
    /// What became of it, which says whether it needs the user's attention.
    pub kind: NoteKind,
    /// The note itself: what was dropped or changed, and what to do about it.
    pub text: String,
}

/// What became of something the source said, from what needs checking to what needs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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
    #[must_use]
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

impl fmt::Display for NoteKind {
    /// The kind's heading ([`NoteKind::title`]).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.title())
    }
}

/// The note on something the source says that the import doesn't read: it is left out, never
/// dropped silently.
pub(crate) const LEFT_OUT: &str = "not read by the import: left out; carry it over by hand if the environment needs it";

/// The notes an import collects as it reads, in the order it reads.
#[derive(Debug, Default)]
pub(crate) struct Notes(Vec<Note>);

impl Notes {
    /// Adds a note on what the source says at `at`.
    pub(crate) fn add(&mut self, at: impl Into<String>, kind: NoteKind, text: impl Into<String>) {
        self.0.push(Note {
            at: at.into(),
            kind,
            text: text.into(),
        });
    }

    /// The finished draft: its YAML, and these notes on it.
    pub(crate) fn draft(self, yaml: String) -> Draft {
        Draft { yaml, notes: self.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_keep_the_order_they_were_read_in() {
        let mut notes = Notes::default();
        notes.add("b", NoteKind::Tooling, "second");
        notes.add("a", NoteKind::Changed, "first");
        let d = notes.draft("version: 1\n".to_owned());
        assert_eq!(d.notes.iter().map(|n| n.at.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(d.yaml, "version: 1\n");
    }

    #[test]
    fn every_kind_is_listed_once_under_its_own_heading() {
        let mut titles: Vec<String> = NoteKind::ALL.iter().map(ToString::to_string).collect();
        titles.sort();
        titles.dedup();
        assert_eq!(titles.len(), NoteKind::ALL.len());
    }
}
