//! Drafting a spec from files people already have. The draft is a starting point: every
//! import also returns notes for what the source said that the draft doesn't.

pub mod compose;
pub mod vagrant;

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
            NoteKind::Tooling => "Compose tooling, nothing to carry over",
        }
    }
}
